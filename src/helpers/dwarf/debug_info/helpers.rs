use gimli::{
    DebuggingInformationEntry, EndianSlice, EntriesCursor, Reader as _, RelocateReader,
    RunTimeEndian, UnitRef, constants,
};

use crate::helpers::dwarf::debug_info::{
    AddressRange, DebugVariable, ExecutionScope, RelocationMap, ScopeCacheNode,
};

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

pub fn extract_subprogram<'a>(
    cursor: &mut EntriesCursor<
        'a,
        RelocateReader<EndianSlice<'a, RunTimeEndian>, &'a RelocationMap>,
    >,
    unit: UnitRef<'a, RelocateReader<EndianSlice<'a, RunTimeEndian>, &'a RelocationMap>>,
) -> Option<ScopeCacheNode> {
    let mut low_pc = None;
    let mut high_pc_attr = None;
    let mut display_name = None;
    let mut linkage_name = None;

    let mut bytes = None;

    let entry = cursor.current().unwrap().clone();

    for attr in entry.attrs() {
        match attr.name() {
            gimli::DW_AT_low_pc => {
                if let gimli::AttributeValue::Addr(addr) = attr.value() {
                    low_pc = Some(addr);
                }
            }
            gimli::DW_AT_high_pc => {
                high_pc_attr = Some(attr.value());
            }
            gimli::DW_AT_name => {
                if let Ok(str) = unit.attr_string(attr.value()) {
                    display_name = Some(str.to_string_lossy().unwrap().to_string());
                }
            }
            gimli::DW_AT_linkage_name => {
                if let Ok(str) = unit.attr_string(attr.value()) {
                    linkage_name = Some(str.to_string_lossy().unwrap().to_string());
                }
            }
            gimli::DW_AT_frame_base => {
                if let gimli::AttributeValue::Exprloc(expression) = attr.value() {
                    bytes = Some(expression.0.inner().to_vec());
                }
            }
            _ => continue,
        }
    }

    let mut variables = Vec::new();
    let mut children = Vec::new();

    if entry.has_children() {
        let parent_depth = entry.depth();

        while let Some(child_entry) = cursor.next_dfs().unwrap() {
            if child_entry.depth() <= parent_depth {
                break;
            }
            match child_entry.tag() {
                constants::DW_TAG_variable => {
                    let variable = extract_variable(child_entry, unit);

                    if let Some(var) = variable {
                        variables.push(var);
                    }
                }
                constants::DW_TAG_subprogram => {
                    let function = extract_subprogram(cursor, unit);

                    if let Some(func) = function {
                        children.push(func);
                    }
                }
                constants::DW_TAG_inlined_subroutine => {
                    if let Some(inline) = extract_inline(cursor, unit) {
                        children.push(inline);
                    }
                }
                constants::DW_TAG_null => {
                    cursor.next_dfs().unwrap();
                    break;
                }
                _ => continue,
            }
        }
    }

    // Ignore entries where the low_pc is 0
    if low_pc.is_some_and(|pc| pc == 0) {
        return None;
    }

    let mut high_pc = None;
    if let (Some(low), Some(high)) = (low_pc, high_pc_attr) {
        high_pc = match high {
            gimli::AttributeValue::Addr(addr) => Some(addr),
            gimli::AttributeValue::Udata(offset) => Some(low + offset),
            _ => None,
        };
    }

    if let (Some(low_pc), Some(high_pc), Some(display_name), Some(linkage_name)) =
        (low_pc, high_pc, display_name, linkage_name)
    {
        let inlined = ExecutionScope::Function {
            display_name,
            linkage_name,
            bytes,
        };

        let ranges = vec![AddressRange { low_pc, high_pc }];

        let node = ScopeCacheNode {
            scope: inlined,
            offset: entry.offset().0,
            variables,
            ranges,
            children,
        };

        return Some(node);
    }
    None
}

pub fn extract_inline<'a>(
    cursor: &mut EntriesCursor<
        'a,
        RelocateReader<EndianSlice<'a, RunTimeEndian>, &'a RelocationMap>,
    >,
    unit: UnitRef<'a, RelocateReader<EndianSlice<'a, RunTimeEndian>, &'a RelocationMap>>,
) -> Option<ScopeCacheNode> {
    let mut low_pc = None;
    let mut high_pc_attr = None;
    let mut abstract_origin_offset = None;

    let entry = cursor.current().unwrap().clone();

    for attr in entry.attrs() {
        match attr.name() {
            gimli::DW_AT_low_pc => {
                if let gimli::AttributeValue::Addr(addr) = attr.value() {
                    low_pc = Some(addr);
                }
            }
            gimli::DW_AT_high_pc => {
                high_pc_attr = Some(attr.value());
            }
            gimli::DW_AT_abstract_origin => {
                if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                    abstract_origin_offset = Some(offset.0);
                }
            }
            _ => continue,
        }
    }

    let mut variables = Vec::new();
    let mut children = Vec::new();

    if entry.has_children() {
        let parent_depth = cursor.depth();

        while let Some(child_entry) = cursor.next_dfs().unwrap() {
            if child_entry.depth() <= parent_depth {
                break;
            }
            match child_entry.tag() {
                constants::DW_TAG_variable => {
                    let variable = extract_variable(child_entry, unit);

                    if let Some(var) = variable {
                        variables.push(var);
                    }
                }
                constants::DW_TAG_subprogram => {
                    if let Some(function) = extract_subprogram(cursor, unit) {
                        children.push(function);
                    }
                }
                constants::DW_TAG_inlined_subroutine => {
                    if let Some(inline) = extract_inline(cursor, unit) {
                        children.push(inline);
                    }
                }
                constants::DW_TAG_null => {
                    cursor.next_dfs().unwrap();
                    break;
                }
                _ => continue,
            }
        }
    }

    // Ignore entries where the low_pc is 0
    if low_pc.is_some_and(|pc| pc == 0) {
        return None;
    }

    let mut high_pc = None;
    if let (Some(low), Some(high)) = (low_pc, high_pc_attr) {
        high_pc = match high {
            gimli::AttributeValue::Addr(addr) => Some(addr),
            gimli::AttributeValue::Udata(offset) => Some(low + offset),
            _ => None,
        };
    }

    if let (Some(low_pc), Some(high_pc), Some(abstract_origin_offset)) =
        (low_pc, high_pc, abstract_origin_offset)
    {
        let inlined = ExecutionScope::Inlined {
            abstract_origin_offset,
        };

        let ranges = vec![AddressRange { low_pc, high_pc }];

        let node = ScopeCacheNode {
            scope: inlined,
            offset: entry.offset().0,
            variables,
            ranges,
            children,
        };

        return Some(node);
    }

    None
}
