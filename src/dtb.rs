use dtb::{Reader, StructItem};
use core::{include_bytes};

pub fn parse() -> Result<Reader<'static>, &'static str> {
    let blob = include_bytes!("dtb/test.dtb").as_slice();
    let reader = Reader::read(blob).unwrap();

    Ok(reader)
}

pub fn read(reader: &Reader<'_>) {
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