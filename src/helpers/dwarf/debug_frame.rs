use object::{Object, ObjectSection};
use std::{borrow, error};

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
pub fn setup_session_debug_frame(object: &object::File<'_>) -> RawDebugFrame {
    update_session_cache(object).unwrap()
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

    let debug_frame_raw = load_section(gimli::SectionId::EhFrame)?;
    Ok(RawDebugFrame(debug_frame_raw.to_vec()))
}
