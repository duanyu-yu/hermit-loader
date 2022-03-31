use dtb::{Reader, StructItem};

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