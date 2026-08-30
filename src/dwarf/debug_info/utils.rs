use gimli::{DebuggingInformationEntry, Reader as _, UnitRef};

use crate::{
    dwarf::debug_info::{AddressRange, DebugVariable, ExecutionScope, Reader, ScopeCacheNode},
    types::UniqueFileId,
};

pub fn extract_variable<'a>(
    entry: &DebuggingInformationEntry<Reader<'a>, usize>,
    unit: &UnitRef<Reader<'a>>,
) -> Option<DebugVariable> {
    let mut name = None;
    let mut target_type_offset = None;
    let mut location = None;
    let mut line = None;

    for attr in entry.attrs() {
        match attr.name() {
            gimli::DW_AT_name => {
                if let Ok(str) = unit.attr_string(attr.value()) {
                    name = Some(str.to_string_lossy().ok()?.to_string());
                }
            }
            gimli::DW_AT_type => {
                if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                    target_type_offset = offset.to_debug_info_offset(unit).map(|o| o.0);
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
            decl_line: decl_line as u32,
        };

        return Some(debug_var);
    }
    None
}

pub fn extract_subprogram_node<'a>(
    entry: &DebuggingInformationEntry<Reader<'a>, usize>,
    unit: &UnitRef<'a, Reader<'a>>,
) -> Option<ScopeCacheNode> {
    let mut low_pc = None;
    let mut high_pc_attr = None;
    let mut display_name = None;
    let mut linkage_name = None;
    let mut decl_file_idx = None;
    let mut decl_line = None;

    let mut bytes = None;

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
                    display_name = Some(str.to_string_lossy().ok()?.to_string());
                }
            }
            gimli::DW_AT_linkage_name => {
                if let Ok(str) = unit.attr_string(attr.value()) {
                    linkage_name = Some(str.to_string_lossy().ok()?.to_string());
                }
            }
            gimli::DW_AT_frame_base => {
                if let gimli::AttributeValue::Exprloc(expression) = attr.value() {
                    bytes = Some(expression.0.inner().to_vec());
                }
            }
            gimli::DW_AT_decl_file => {
                if let gimli::AttributeValue::FileIndex(idx) = attr.value() {
                    decl_file_idx = Some(idx);
                }
            }
            gimli::DW_AT_decl_line => {
                if let gimli::AttributeValue::Udata(line) = attr.value() {
                    decl_line = Some(line as u32);
                }
            }
            _ => continue,
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

    if let (
        Some(low_pc),
        Some(high_pc),
        Some(display_name),
        Some(linkage_name),
        Some(decl_file_idx),
        Some(decl_line),
    ) = (
        low_pc,
        high_pc,
        display_name,
        linkage_name,
        decl_file_idx,
        decl_line,
    ) {
        let func = ExecutionScope::Function {
            display_name,
            linkage_name,
            bytes,
            decl_file_id: UniqueFileId {
                offset: unit.offset().0,
                file_idx: decl_file_idx,
            },
            decl_line,
        };

        let ranges = vec![AddressRange { low_pc, high_pc }];

        let node = ScopeCacheNode {
            scope: func,
            offset: entry.offset().0,
            variables: Vec::new(),
            args: Vec::new(),
            ranges,
            children: Vec::new(),
        };

        return Some(node);
    }

    None
}

pub fn extract_inline_node<'a>(
    entry: &DebuggingInformationEntry<Reader<'a>, usize>,
    unit: &UnitRef<'a, Reader<'a>>,
) -> Option<ScopeCacheNode> {
    let mut low_pc = None;
    let mut high_pc_attr = None;
    let mut call_file_idx = None;
    let mut call_line = None;

    let mut range_list_offset = None;
    let mut abstract_origin = None;

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
            gimli::DW_AT_call_file => {
                if let gimli::AttributeValue::FileIndex(idx) = attr.value() {
                    call_file_idx = Some(idx);
                }
            }
            gimli::DW_AT_call_line => {
                if let gimli::AttributeValue::Udata(cl) = attr.value() {
                    call_line = Some(cl as u32);
                }
            }
            gimli::DW_AT_abstract_origin => {
                if let gimli::AttributeValue::DebugInfoRef(offset) = attr.value() {
                    let unit_off = offset.to_unit_offset(unit);

                    abstract_origin = unit_off;
                }

                if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                    abstract_origin = Some(offset);
                }
            }
            gimli::DW_AT_ranges => {
                if let gimli::AttributeValue::RangeListsRef(offset) = attr.value() {
                    range_list_offset = Some(offset);
                }
            }
            _ => continue,
        }
    }

    // Ignore entries where the low_pc is 0
    if low_pc.is_some_and(|pc| pc == 0) {
        return None;
    }

    let mut name = None;
    let mut decl_file = None;
    let mut decl_line = None;

    if let Some(offset) = abstract_origin {
        let entry = unit.entry(offset).ok()?;

        for attr in entry.attrs() {
            match attr.name() {
                gimli::DW_AT_specification => {
                    if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                        let entry = unit.entry(offset).ok()?;

                        let name_attr = entry.attr(gimli::DW_AT_name)?;

                        if let Ok(str) = unit.attr_string(name_attr.value()) {
                            name = Some(str.to_string_lossy().ok()?.to_string());
                        }

                        let decl_file_attr = entry.attr(gimli::DW_AT_decl_file)?;

                        if let gimli::AttributeValue::FileIndex(idx) = decl_file_attr.value() {
                            decl_file = Some(idx);
                        }

                        let decl_line_attr = entry.attr(gimli::DW_AT_decl_line)?;

                        if let gimli::AttributeValue::Udata(dl) = decl_line_attr.value() {
                            decl_line = Some(dl as u32);
                        }

                        break;
                    }
                }
                gimli::DW_AT_name => {
                    if let Ok(str) = unit.attr_string(attr.value()) {
                        name = Some(str.to_string_lossy().ok()?.to_string());
                    }
                }
                gimli::DW_AT_decl_file => {
                    if let gimli::AttributeValue::FileIndex(idx) = attr.value() {
                        decl_file = Some(idx);
                    }
                }
                gimli::DW_AT_decl_line => {
                    if let gimli::AttributeValue::Udata(dl) = attr.value() {
                        decl_line = Some(dl as u32);
                    }
                }
                _ => continue,
            }
        }
    }

    let mut high_pc = None;
    if let (Some(low), Some(high)) = (low_pc, high_pc_attr) {
        high_pc = match high {
            gimli::AttributeValue::Addr(addr) => Some(addr),
            gimli::AttributeValue::Udata(offset) => Some(low + offset),
            _ => None,
        };
    }

    let mut block_ranges = Vec::new();
    if let Some(range_list_offset) = range_list_offset {
        let offset = unit.ranges_offset_from_raw(range_list_offset);
        if let Ok(mut range_iter) = unit.ranges(offset) {
            while let Ok(Some(range)) = range_iter.next() {
                block_ranges.push(AddressRange {
                    low_pc: range.begin,
                    high_pc: range.end,
                });
            }
        }

        if let (
            Some(name),
            Some(call_file_idx),
            Some(call_line),
            Some(decl_file_idx),
            Some(decl_line),
        ) = (name.clone(), call_file_idx, call_line, decl_file, decl_line)
        {
            let inlined = ExecutionScope::Inlined {
                name,
                unit_offset: unit.offset().0,
                call_file_idx,
                decl_file_idx,
                call_line,
                decl_line,
            };

            let node = ScopeCacheNode {
                scope: inlined,
                offset: entry.offset().0,
                variables: Vec::new(),
                args: Vec::new(),
                ranges: block_ranges,
                children: Vec::new(),
            };

            return Some(node);
        }
    }

    if let (
        Some(name),
        Some(low_pc),
        Some(high_pc),
        Some(call_file_idx),
        Some(call_line),
        Some(decl_file_idx),
        Some(decl_line),
    ) = (
        name,
        low_pc,
        high_pc,
        call_file_idx,
        call_line,
        decl_file,
        decl_line,
    ) {
        let inlined = ExecutionScope::Inlined {
            name,
            unit_offset: unit.offset().0,
            call_file_idx,
            decl_file_idx,
            call_line,
            decl_line,
        };

        let ranges = vec![AddressRange { low_pc, high_pc }];

        let node = ScopeCacheNode {
            scope: inlined,
            offset: entry.offset().0,
            variables: Vec::new(),
            args: Vec::new(),
            ranges,
            children: Vec::new(),
        };

        return Some(node);
    }

    None
}

pub fn extract_lexical_block_node<'a>(
    entry: &DebuggingInformationEntry<Reader<'a>, usize>,
    unit: &UnitRef<'a, Reader<'a>>,
) -> Option<ScopeCacheNode> {
    let mut low_pc = None;
    let mut high_pc_attr = None;
    let mut range_list_offset = None;
    let mut block_ranges = Vec::new();

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
            gimli::DW_AT_ranges => {
                if let gimli::AttributeValue::RangeListsRef(offset) = attr.value() {
                    range_list_offset = Some(offset);
                }
            }
            _ => continue,
        }
    }

    if low_pc.is_some_and(|pc| pc == 0) {
        return None;
    }

    if let Some(range_list_offset) = range_list_offset {
        let offset = unit.ranges_offset_from_raw(range_list_offset);
        if let Ok(mut range_iter) = unit.ranges(offset) {
            while let Ok(Some(range)) = range_iter.next() {
                block_ranges.push(AddressRange {
                    low_pc: range.begin,
                    high_pc: range.end,
                });
            }
        }

        let lexical_block = ExecutionScope::LexicalBlock;

        let node = ScopeCacheNode {
            scope: lexical_block,
            offset: entry.offset().0,
            variables: Vec::new(),
            args: Vec::new(),
            ranges: block_ranges,
            children: Vec::new(),
        };

        return Some(node);
    }

    let mut high_pc = None;
    if let (Some(low), Some(high)) = (low_pc, high_pc_attr) {
        high_pc = match high {
            gimli::AttributeValue::Addr(addr) => Some(addr),
            gimli::AttributeValue::Udata(offset) => Some(low + offset),
            _ => None,
        };
    }

    if let (Some(low_pc), Some(high_pc)) = (low_pc, high_pc) {
        let lexical_block = ExecutionScope::LexicalBlock;

        let ranges = vec![AddressRange { low_pc, high_pc }];

        let node = ScopeCacheNode {
            scope: lexical_block,
            offset: entry.offset().0,
            variables: Vec::new(),
            args: Vec::new(),
            ranges,
            children: Vec::new(),
        };

        return Some(node);
    }

    None
}
