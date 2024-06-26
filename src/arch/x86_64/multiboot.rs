use alloc::format;
use core::ptr::write_bytes;
use core::{mem, ptr, slice};

use align_address::Align;
use hermit_entry::boot_info::{
	BootInfo, DeviceTreeAddress, HardwareInfo, PlatformInfo, SerialPortBase,
};
use hermit_entry::elf::LoadedKernel;
use log::info;
use multiboot::information::{MemoryManagement, MemoryType, Multiboot, PAddr};
use sptr::Strict;
use vm_fdt::{FdtWriter, FdtWriterResult};
use x86_64::structures::paging::{PageSize, PageTableFlags, Size2MiB, Size4KiB};

use super::paging;
use super::physicalmem::PhysAlloc;
use crate::arch::x86_64::{KERNEL_STACK_SIZE, SERIAL_IO_PORT};
use crate::BootInfoExt;

extern "C" {
	static loader_end: u8;
	static mb_info: usize;
}

#[allow(bad_asm_style)]
mod entry {
	core::arch::global_asm!(include_str!("entry.s"));
}

struct Mem;

impl MemoryManagement for Mem {
	unsafe fn paddr_to_slice<'a>(&self, p: PAddr, sz: usize) -> Option<&'static [u8]> {
		let ptr = sptr::from_exposed_addr(p as usize);
		unsafe { Some(slice::from_raw_parts(ptr, sz)) }
	}

	// If you only want to read fields, you can simply return `None`.
	unsafe fn allocate(&mut self, _length: usize) -> Option<(PAddr, &mut [u8])> {
		None
	}

	unsafe fn deallocate(&mut self, addr: PAddr) {
		if addr != 0 {
			unimplemented!()
		}
	}
}

pub struct DeviceTree;

impl DeviceTree {
	pub fn create() -> FdtWriterResult<&'static [u8]> {
		let mut mem = Mem;
		let multiboot = unsafe { Multiboot::from_ptr(mb_info as u64, &mut mem).unwrap() };

		let all_regions = multiboot
			.memory_regions()
			.expect("Could not find a memory map in the Multiboot information");
		let ram_regions = all_regions.filter(|m| m.memory_type() == MemoryType::Available);

		let mut fdt = FdtWriter::new()?;

		let root_node = fdt.begin_node("")?;
		fdt.property_string("compatible", "linux,dummy-virt")?;
		fdt.property_u32("#address-cells", 0x2)?;
		fdt.property_u32("#size-cells", 0x2)?;

		if let Some(cmdline) = multiboot.command_line() {
			let chosen_node = fdt.begin_node("chosen")?;
			fdt.property_string("bootargs", cmdline)?;
			fdt.end_node(chosen_node)?;
		}

		for m in ram_regions {
			let start_address = m.base_address();
			let length = m.length();

			let memory_node = fdt.begin_node(format!("memory@{:x}", start_address).as_str())?;
			fdt.property_string("device_type", "memory")?;
			fdt.property_array_u64("reg", &[start_address, length])?;
			fdt.end_node(memory_node)?;
		}

		fdt.end_node(root_node)?;

		let fdt = fdt.finish()?;

		Ok(fdt.leak())
	}
}

pub fn find_kernel() -> &'static [u8] {
	use core::cmp;

	paging::clean_up();
	// Identity-map the Multiboot information.
	unsafe {
		assert!(mb_info > 0, "Could not find Multiboot information");
		info!("Found Multiboot information at {:#x}", mb_info);
	}
	let page_address = unsafe { mb_info.align_down(Size4KiB::SIZE as usize) };
	paging::map::<Size4KiB>(page_address, page_address, 1, PageTableFlags::empty());

	let mut mem = Mem;
	// Load the Multiboot information and identity-map the modules information.
	let multiboot = unsafe { Multiboot::from_ptr(mb_info as u64, &mut mem).unwrap() };
	let modules_address = multiboot
		.modules()
		.expect("Could not find a memory map in the Multiboot information")
		.next()
		.expect("Could not find first map address")
		.start as usize;
	let page_address = modules_address.align_down(Size4KiB::SIZE as usize);
	paging::map::<Size4KiB>(page_address, page_address, 1, PageTableFlags::empty());

	// Iterate through all modules.
	// Collect the start address of the first module and the highest end address of all modules.
	let modules = multiboot.modules().unwrap();
	let mut found_module = false;
	let mut start_address = 0;
	let mut end_address = 0;

	for m in modules {
		found_module = true;

		if start_address == 0 {
			start_address = m.start as usize;
		}

		if m.end as usize > end_address {
			end_address = m.end as usize;
		}
	}

	info!("Found module: [{:#x} - {:#x}]", start_address, end_address);
	let elf_start = start_address;
	let elf_len = end_address - start_address;
	info!("Module length: {:#x}", elf_len);

	let free_memory_address = end_address.align_up(Size2MiB::SIZE as usize);
	// TODO: Workaround for https://github.com/hermitcore/loader/issues/96
	let free_memory_address = cmp::max(free_memory_address, 0x800000);
	// Memory after the highest end address is unused and available for the physical memory manager.
	PhysAlloc::init(free_memory_address);

	// Identity-map the ELF header of the first module.
	assert!(
		found_module,
		"Could not find a single module in the Multiboot information"
	);
	assert!(start_address > 0);
	info!("Found an ELF module at {:#x}", start_address);
	let page_address = start_address.align_down(Size4KiB::SIZE as usize) + Size4KiB::SIZE as usize;
	let counter =
		(start_address.align_up(Size2MiB::SIZE as usize) - page_address) / Size4KiB::SIZE as usize;
	paging::map::<Size4KiB>(page_address, page_address, counter, PageTableFlags::empty());

	// map also the rest of the module
	let address = start_address.align_up(Size2MiB::SIZE as usize);
	let counter =
		(end_address.align_up(Size2MiB::SIZE as usize) - address) / Size2MiB::SIZE as usize;
	if counter > 0 {
		paging::map::<Size2MiB>(address, address, counter, PageTableFlags::empty());
	}

	unsafe { slice::from_raw_parts(sptr::from_exposed_addr(elf_start), elf_len) }
}

pub unsafe fn boot_kernel(kernel_info: LoadedKernel) -> ! {
	let LoadedKernel {
		load_info,
		entry_point,
	} = kernel_info;

	let mut mem = Mem;
	let multiboot = unsafe { Multiboot::from_ptr(mb_info as u64, &mut mem).unwrap() };

	// determine boot stack address
	let mut new_stack = unsafe { ptr::addr_of!(loader_end) }
		.addr()
		.align_up(Size4KiB::SIZE as usize);

	if new_stack + KERNEL_STACK_SIZE as usize > unsafe { mb_info } {
		new_stack = (unsafe { mb_info } + mem::size_of::<Multiboot<'_, '_>>())
			.align_up(Size4KiB::SIZE as usize);
	}

	let command_line = multiboot.command_line();
	if let Some(command_line) = command_line {
		let cmdline = command_line.as_ptr() as usize;
		let cmdsize = command_line.len();
		if new_stack + KERNEL_STACK_SIZE as usize > cmdline {
			new_stack = (cmdline + cmdsize).align_up(Size4KiB::SIZE as usize);
		}
	}

	// map stack in the address space
	paging::map::<Size4KiB>(
		new_stack,
		new_stack,
		KERNEL_STACK_SIZE as usize / Size4KiB::SIZE as usize,
		PageTableFlags::WRITABLE,
	);

	// clear stack
	unsafe {
		write_bytes(
			sptr::from_exposed_addr_mut::<u8>(new_stack),
			0,
			KERNEL_STACK_SIZE.try_into().unwrap(),
		);
	}

	let device_tree = DeviceTree::create().expect("Unable to create devicetree!");

	let boot_info = BootInfo {
		hardware_info: HardwareInfo {
			phys_addr_range: 0..0,
			serial_port_base: SerialPortBase::new(SERIAL_IO_PORT),
			device_tree: DeviceTreeAddress::new(device_tree.as_ptr() as u64),
		},
		load_info,
		platform_info: PlatformInfo::Multiboot {
			command_line,
			multiboot_info_addr: (unsafe { mb_info } as u64).try_into().unwrap(),
		},
	};

	let stack = sptr::from_exposed_addr_mut(new_stack);
	let entry = sptr::from_exposed_addr(entry_point.try_into().unwrap());
	let raw_boot_info = boot_info.write();

	unsafe { super::enter_kernel(stack, entry, raw_boot_info) }
}
