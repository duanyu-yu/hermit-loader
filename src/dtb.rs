use dtb::{Reader, StructItem};
use multiboot::information::{Multiboot};
use alloc::vec::Vec;
use alloc::string::String;
use crate::arch::x86_64::MEM;
use crate::devicetree::{DeviceTree, DeviceTreeProperty};

extern "C" {
    static mb_info: usize;
}

pub fn parse() -> Result<Reader<'static>, &'static str> {
    let blob: &[u8] = include_bytes_aligned!(64, "dtb/basic.dtb");
    let reader = Reader::read(blob).unwrap();

    Ok(reader)
}

pub fn read(reader: &Reader<'_>) {
    loaderlog!("Device Tree:");

    for entry in reader.reserved_mem_entries(){
        loaderlog!("reserved: {:#X?}, {:#X?}", entry.address, entry.size);
    }

    let mut indent = 0;
    for entry in reader.struct_items() {
        match entry {
            StructItem::BeginNode { name } => {
                loaderlog!("{:indent$}{} {{", "", name, indent = indent);
                indent += 2;
            }
            StructItem::EndNode => {
                indent -= 2;
                loaderlog!("{:indent$}}}", "", indent = indent);
            }
            StructItem::Property { name, value } => {
                loaderlog!("{:indent$}{}: {:?}", "", name, value, indent = indent)
            }
        }
    }
}

pub fn read_from_address(addr: usize) {
    let reader = unsafe { Reader::read_from_address(addr).unwrap() };

    for entry in reader.reserved_mem_entries(){
        loaderlog!("reserved: {:#X?}, {:#X?}", entry.address, entry.size);
    }

    let mut indent = 0;
    for entry in reader.struct_items() {
        match entry {
            StructItem::BeginNode { name } => {
                loaderlog!("{:indent$}{} {{", "", name, indent = indent);
                indent += 2;
            }
            StructItem::EndNode => {
                indent -= 2;
                loaderlog!("{:indent$}}}", "", indent = indent);
            }
            StructItem::Property { name, value } => {
                loaderlog!("{:indent$}{}: {:?}", "", name, value, indent = indent)
            }
        }
    }
}

pub unsafe fn from_mb() -> Result<DeviceTree, &'static str> {
    assert!(mb_info > 0, "Could not find Multiboot information");
	loaderlog!("Found Multiboot information at {:#x}", mb_info);

    // Load the Multiboot information
    let multiboot = Multiboot::from_ptr(mb_info as u64, &mut MEM).unwrap();
    let memory_regions = multiboot
        .memory_regions()
        .expect("Could not find a memory map in the Multiboot information");

    let mut reg: Vec<u32> = Vec::new();
    
    for m in memory_regions {
        reg.push(m.base_address() as u32);
        reg.push(m.length() as u32); 
    }

    let mut dt = DeviceTree::new();

    dt.edit_property(&String::from("memory"), &String::from("reg"), DeviceTreeProperty::MultipleUnsignedInt32(reg));

    Ok(dt)
}
