use gimli::{
    DebuggingInformationEntry, EndianSlice, Reader as _, RelocateReader, RunTimeEndian, UnitRef,
};

use crate::helpers::dwarf::debug_info::{DebugVariable, RelocationMap};

pub fn extract_variable<'a>(
    entry: &DebuggingInformationEntry<
        RelocateReader<EndianSlice<'a, RunTimeEndian>, &'a RelocationMap>,
        usize,
    >,
    unit: UnitRef<'a, RelocateReader<EndianSlice<'a, RunTimeEndian>, &'a RelocationMap>>,
) -> Option<DebugVariable> {
    let mut name = None;
    let mut target_type_offset = None;
    let mut location = None;
    let mut line = None;

    for attr in entry.attrs() {
        match attr.name() {
            gimli::DW_AT_name => {
                if let Ok(str) = unit.attr_string(attr.value()) {
                    name = Some(str.to_string_lossy().unwrap().to_string());
                }
            }
            gimli::DW_AT_type => {
                if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                    target_type_offset = Some(offset.0);
                }
            }
            gimli::DW_AT_location => {
                if let gimli::AttributeValue::Exprloc(expression) = attr.value() {
                    let slice = expression.0.inner();
                    location = Some(slice.to_vec());
                }
            }
            gimli::DW_AT_decl_line => {
                if let gimli::AttributeValue::Udata(decl_line) = attr.value() {
                    line = Some(decl_line as u64);
                }
            }
            _ => continue,
        }
    }

    if let (Some(name), Some(target_type_offset), Some(location), Some(decl_line)) =
        (name, target_type_offset, location, line)
    {
        let debug_var = DebugVariable {
            name,
            target_type_offset,
            location,
            decl_line,
        };

        return Some(debug_var);
    }
    None
}
