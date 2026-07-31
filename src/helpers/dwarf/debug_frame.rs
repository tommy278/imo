use object::{Object, ObjectSection};
use std::{
    borrow, error,
    fs::{self},
};

/*
 * Based on the 'simple_line' example from the gimli project.
 * Source: https://github.com/gimli-rs/gimli/blob/main/crates/examples/src/bin/simple_line.rs
 * * The implementation below was adapted to support specific address lookup
 * and integrated into the project's native debugger architecture.
 */

#[derive(Debug, Default, Clone)]
pub struct RawDebugFrame(pub Vec<u8>);

impl RawDebugFrame {
    pub fn get_unwind_table_with_endian(
        &self,
        endian: gimli::RunTimeEndian,
    ) -> gimli::EhFrame<gimli::EndianSlice<'_, gimli::RunTimeEndian>> {
        gimli::EhFrame::new(&self.0, endian)
    }
}

/// Get details from dwarf and apply them to the debug session cache
pub fn setup_session_debug_frame(binary_path: &str) -> RawDebugFrame {
    let file = fs::File::open(binary_path).unwrap();
    // TODO: Find a safer way to map file as suggested by gimli
    let mmap = unsafe { memmap2::Mmap::map(&file).unwrap() };
    let object = object::File::parse(&*mmap).unwrap();
    update_session_cache(&object).unwrap()
}

/// Update the BreakpointTarget and SourceLocation in the debug session cache
fn update_session_cache(object: &object::File) -> Result<RawDebugFrame, Box<dyn error::Error>> {
    // Load a section and return as `Cow<[u8]>`.
    let load_section = |id: gimli::SectionId| -> Result<borrow::Cow<[u8]>, Box<dyn error::Error>> {
        Ok(match object.section_by_name(id.name()) {
            Some(section) => section.uncompressed_data()?,
            None => borrow::Cow::Borrowed(&[]),
        })
    };

    // Borrow a `Cow<[u8]>` to create an `EndianSlice`.
    // let borrow_section = |section| gimli::EndianSlice::new(borrow::Cow::as_ref(section), endian);

    let debug_frame_raw = load_section(gimli::SectionId::EhFrame)?;
    // let debug_frame = gimli::DebugFrame::new(&debug_frame_raw, endian);

    // let base_addresses = gimli::BaseAddresses::default();
    // let mut entries = debug_frame.entries(&base_addresses);

    // while let Ok(Some(entry)) = entries.next() {
    //     println!("{:?}", entry);
    // }
    Ok(RawDebugFrame(debug_frame_raw.to_vec()))
}
