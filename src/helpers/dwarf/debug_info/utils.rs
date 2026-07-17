/*
 * Based on the 'simple.rs' example from the gimli project.
 * Source: https://github.com/gimli-rs/gimli/blob/main/crates/examples/src/bin/simple.rs
 * * The implementation below was adapted to support specific address lookup
 * and integrated into the project's native debugger architecture.
 */

use gimli::{EndianSlice, Range, Reader as _, RelocateReader, RunTimeEndian, UnitRef, constants};
use nix::libc::execl;
use object::{Object, ObjectSection};
use std::{borrow, error, fs};

use crate::helpers::dwarf::debug_info::{
    Abi, AddressRange, DebuggerMetadataCache, DwarfType, EnumVariant, Enumerator, ExecutionScope,
    GenericField, Reader, RelocationMap, ScopeCacheNode, StructField, TypeCacheNode,
    helpers::{extract_inline, extract_lexical_block, extract_subprogram, extract_variable},
};

// The section data that will be stored in `DwarfSections` and `DwarfPackageSections`.
#[derive(Default)]
struct Section<'data> {
    data: borrow::Cow<'data, [u8]>,
    relocations: RelocationMap,
}

// The reader type that will be stored in `Dwarf` and `DwarfPackage`.
// If you don't need relocations, you can use `gimli::EndianSlice` directly.
pub fn lookup_vars(binary_path: &str, info_cache: &mut DebuggerMetadataCache) {
    let file = fs::File::open(binary_path).unwrap();
    // SAFETY: This is not safe. `gimli` does not mitigate against modifications to the
    // file while it is being read. See the `memmap2` documentation and take your own
    // precautions. `fs::read` could be used instead if you don't mind loading the entire
    // file into memory.
    let mmap = unsafe { memmap2::Mmap::map(&file).unwrap() };
    let object = object::File::parse(&*mmap).unwrap();

    info_cache.abi = Abi::new(object.format());

    // Update endian in info_cache
    info_cache.endian = if object.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };

    dump_file(&object, info_cache).unwrap();
}

fn dump_file(
    object: &object::File,
    info_cache: &mut DebuggerMetadataCache,
) -> Result<(), Box<dyn error::Error>> {
    // Load a `Section` that may own its data.
    fn load_section<'data>(
        object: &object::File<'data>,
        name: &str,
    ) -> Result<Section<'data>, Box<dyn error::Error>> {
        Ok(match object.section_by_name(name) {
            Some(section) => Section {
                data: section.uncompressed_data()?,
                relocations: section.relocation_map().map(RelocationMap)?,
            },
            None => Default::default(),
        })
    }

    // Borrow a `Section` to create a `Reader`.
    fn borrow_section<'data>(
        section: &'data Section<'data>,
        endian: gimli::RunTimeEndian,
    ) -> Reader<'data> {
        let slice = gimli::EndianSlice::new(borrow::Cow::as_ref(&section.data), endian);
        gimli::RelocateReader::new(slice, &section.relocations)
    }

    // Load all of the sections.
    let dwarf_sections = gimli::DwarfSections::load(|id| load_section(object, id.name()))?;

    // Create `Reader`s for all of the sections and do preliminary parsing.
    // Alternatively, we could have used `Dwarf::load` with an owned type such as `EndianRcSlice`.
    let dwarf = dwarf_sections.borrow(|section| borrow_section(section, info_cache.endian));
    // Iterate over the compilation units.
    let mut iter = dwarf.units();
    while let Some(header) = iter.next()? {
        // println!("Unit at <.debug_info+0x{:x}>", header.offset().0);
        let unit = dwarf.unit(header)?;
        let unit_ref = unit.unit_ref(&dwarf);
        dump_unit(unit_ref, info_cache)?;
    }

    Ok(())
}

fn dump_unit<'a>(
    unit: UnitRef<'a, RelocateReader<EndianSlice<'a, RunTimeEndian>, &'a RelocationMap>>,
    info_cache: &mut DebuggerMetadataCache,
) -> Result<(), gimli::Error> {
    // Update encoding in cache
    if info_cache.encoding.is_none() {
        info_cache.encoding = Some(unit.encoding());
    }

    // Iterate over the Debugging Information Entries (DIEs) in the unit.
    let mut entries = unit.entries();

    // NOTE: For some reason first entry is always none, so skip it to loop safely
    entries.next_dfs()?;

    while let Some(entry) = entries.current() {
        let offset = entry.offset().0;

        // Parse each entries for the needed values while skipping redundant values
        match entry.tag() {
            constants::DW_TAG_base_type => {
                let mut name: Option<String> = None;
                let mut encoding = None;
                let mut byte_size = None;

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_byte_size => {
                            byte_size = attr.value().udata_value();
                        }
                        gimli::DW_AT_encoding => {
                            if let gimli::AttributeValue::Encoding(enc) = attr.value() {
                                encoding = Some(enc.0);
                            }
                        }
                        gimli::DW_AT_name => {
                            if let Ok(str) = unit.attr_string(attr.value()) {
                                name = Some(str.to_string_lossy().unwrap().to_string())
                            }
                        }
                        _ => {
                            continue;
                        }
                    }
                }

                if let (Some(name), Some(encoding), Some(byte_size)) = (name, encoding, byte_size) {
                    let base_type = DwarfType::Base {
                        name,
                        encoding,
                        byte_size,
                    };
                    let cache_node = TypeCacheNode {
                        dwarf_type: base_type,
                        offset,
                    };
                    info_cache.type_index.insert(offset, cache_node);
                }

                entries.next_dfs()?;
            }
            constants::DW_TAG_pointer_type => {
                let mut target_type_offset = None;
                let mut name = None;

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_type => {
                            if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                                target_type_offset = Some(offset.0);
                            }
                        }
                        gimli::DW_AT_name => {
                            if let Ok(str) = unit.attr_string(attr.value()) {
                                name = Some(str.to_string_lossy().unwrap().to_string());
                            }
                        }
                        _ => continue,
                    }
                }

                if let Some(target_type_offset) = target_type_offset {
                    let pointer_type = DwarfType::Pointer {
                        name,
                        target_type_offset,
                    };
                    let cache_node = TypeCacheNode {
                        dwarf_type: pointer_type,
                        offset,
                    };
                    info_cache.type_index.insert(offset, cache_node);
                }

                entries.next_dfs()?;
            }
            constants::DW_TAG_const_type => {
                let mut target_type_offset = None;

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_type => {
                            if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                                target_type_offset = Some(offset.0);
                            }
                        }
                        _ => continue,
                    }
                }

                if let Some(tto) = target_type_offset {
                    let const_type = DwarfType::Const {
                        target_type_offset: tto,
                    };
                    let cache_node = TypeCacheNode {
                        dwarf_type: const_type,
                        offset,
                    };
                    info_cache.type_index.insert(offset, cache_node);
                }

                entries.next_dfs()?;
            }
            constants::DW_TAG_enumeration_type => {
                let mut name = None;
                let mut byte_size = None;
                let mut fields = Vec::new();

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_name => {
                            if let Ok(str) = unit.attr_string(attr.value()) {
                                name = Some(str.to_string_lossy().unwrap().to_string());
                            }
                        }
                        gimli::DW_AT_byte_size => {
                            if let gimli::AttributeValue::Udata(size) = attr.value() {
                                byte_size = Some(size);
                            }
                        }
                        _ => continue,
                    }
                }

                if entry.has_children() {
                    let mut cursor = unit.entries_at_offset(entry.offset()).unwrap();

                    cursor.next_dfs().unwrap();

                    let start_depth = cursor.current().map(|e| e.depth()).unwrap_or_default();

                    while let Some(child_entry) = cursor.next_dfs().unwrap() {
                        if child_entry.depth() <= start_depth {
                            break;
                        }

                        let mut inner_name = None;
                        let mut const_value = None;

                        match child_entry.tag() {
                            gimli::DW_TAG_enumerator => {
                                for attr in child_entry.attrs() {
                                    match attr.name() {
                                        gimli::DW_AT_name => {
                                            if let Ok(str) = unit.attr_string(attr.value()) {
                                                inner_name = Some(
                                                    str.to_string_lossy().unwrap().to_string(),
                                                );
                                            }
                                        }
                                        gimli::DW_AT_const_value => {
                                            if let gimli::AttributeValue::Udata(value) =
                                                attr.value()
                                            {
                                                const_value = Some(value);
                                            }
                                        }
                                        _ => continue,
                                    }
                                }
                            }
                            _ => continue,
                        }

                        if let (Some(name), Some(value)) = (inner_name, const_value) {
                            fields.push(Enumerator { name, value });
                        }
                    }
                }

                if let (Some(name), Some(byte_size)) = (name, byte_size) {
                    let enum_type = DwarfType::Enum {
                        name,
                        byte_size,
                        fields,
                    };

                    let cache_node = TypeCacheNode {
                        dwarf_type: enum_type,
                        offset,
                    };

                    info_cache.type_index.insert(offset, cache_node);
                }

                entries.next_dfs()?;
            }
            constants::DW_TAG_array_type => {
                let mut target_type_offset = None;

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_type => {
                            if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                                target_type_offset = Some(offset);
                            }
                        }
                        _ => continue,
                    }
                }

                let mut element_count = None;

                if entry.has_children() {
                    let mut cursor = unit.entries_at_offset(entry.offset()).unwrap();

                    // Skip array entry, move to first child
                    cursor.next_dfs().unwrap();

                    while let Some(entry) = cursor.next_dfs().unwrap() {
                        if entry.tag() == constants::DW_TAG_subrange_type {
                            for attr in entry.attrs() {
                                match attr.name() {
                                    gimli::DW_AT_count => {
                                        element_count = match attr.value() {
                                            gimli::AttributeValue::Data1(v) => Some(v as u64),
                                            _ => None,
                                        };
                                    }
                                    _ => continue,
                                }
                            }
                            break;
                        }
                    }
                    if let (Some(target_type_offset), Some(count)) =
                        (target_type_offset, element_count)
                    {
                        let array_type = DwarfType::Array {
                            target_type_offset: target_type_offset.0,
                            count,
                        };

                        let cache_node = TypeCacheNode {
                            dwarf_type: array_type,
                            offset,
                        };

                        info_cache.type_index.insert(offset, cache_node);
                    }
                }

                entries.next_dfs()?;
            }
            constants::DW_TAG_structure_type => {
                let mut name = None;
                let mut byte_size = None;
                let mut alignment = None;

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_name => {
                            if let Ok(str) = unit.attr_string(attr.value()) {
                                name = Some(str.to_string_lossy().unwrap().to_string());
                            }
                        }
                        gimli::DW_AT_byte_size => {
                            if let gimli::AttributeValue::Udata(size) = attr.value() {
                                byte_size = Some(size);
                            }
                        }
                        gimli::DW_AT_alignment => {
                            if let gimli::AttributeValue::Udata(ali) = attr.value() {
                                alignment = Some(ali);
                            }
                        }
                        _ => continue,
                    }
                }

                let mut is_enum = false;
                let mut discr_member_offset = None;

                // Storage for each enum variants
                let mut enum_variants = Vec::new();

                // Fallback for Niche Optimized Variants that are not captured under the variant path
                // Also doubles down as storage for regular structs
                let mut fallback_fields = Vec::new();

                // Storage for generic fields for example Vec<T> will produce T and the type it belongs to eg. i32
                let mut generics: Vec<GenericField> = Vec::new();

                if entry.has_children() {
                    // Iterate through children
                    let mut cursor = unit.entries_at_offset(entry.offset()).unwrap();
                    cursor.next_dfs().unwrap(); // Skip first since it is the current entry
                    let start_depth = cursor.current().map(|e| e.depth()).unwrap_or_default();

                    while let Some(child_entry) = cursor.next_dfs().unwrap() {
                        if child_entry.depth() <= start_depth {
                            break;
                        }

                        match child_entry.tag() {
                            // Capture fields that are in the Niche Optimized Variants
                            gimli::DW_TAG_member => {
                                let mut type_offset = None;
                                let mut location = None;
                                let mut name = None;

                                for attr in child_entry.attrs() {
                                    match attr.name() {
                                        gimli::DW_AT_name => {
                                            if let Ok(str) = unit.attr_string(attr.value()) {
                                                name = Some(
                                                    str.to_string_lossy().unwrap().to_string(),
                                                );
                                            }
                                        }
                                        gimli::DW_AT_type => {
                                            if let gimli::AttributeValue::UnitRef(offset) =
                                                attr.value()
                                            {
                                                type_offset = Some(offset.0);
                                            }
                                        }
                                        gimli::DW_AT_data_member_location => {
                                            if let gimli::AttributeValue::Udata(loc) = attr.value()
                                            {
                                                location = Some(loc);
                                            }
                                        }
                                        _ => continue,
                                    }
                                }

                                if let (Some(name), Some(type_offset), Some(location)) =
                                    (name, type_offset, location)
                                {
                                    let field = StructField {
                                        name,
                                        type_offset,
                                        location,
                                    };

                                    fallback_fields.push(field);
                                }
                            }
                            gimli::DW_TAG_template_type_parameter => {
                                let mut name = None;
                                let mut type_offset = None;

                                for attr in child_entry.attrs() {
                                    match attr.name() {
                                        gimli::DW_AT_name => {
                                            if let Ok(str) = unit.attr_string(attr.value()) {
                                                name = Some(
                                                    str.to_string_lossy().unwrap().to_string(),
                                                );
                                            }
                                        }
                                        gimli::DW_AT_type => {
                                            if let gimli::AttributeValue::UnitRef(offset) =
                                                attr.value()
                                            {
                                                type_offset = Some(offset.0);
                                            }
                                        }
                                        _ => continue,
                                    }
                                }

                                if let (Some(name), Some(type_offset)) = (name, type_offset) {
                                    if !generics.iter().any(|g| g.name == name) {
                                        generics.push(GenericField { name, type_offset });
                                    }
                                }
                            }
                            // Descend to the variant part
                            gimli::DW_TAG_variant_part => {
                                is_enum = true;

                                if let Some(gimli::AttributeValue::UnitRef(discr_offset)) =
                                    child_entry.attr_value(gimli::DW_AT_discr)
                                {
                                    let discr_entry = unit.entry(discr_offset).unwrap();
                                    if let Some(gimli::AttributeValue::Udata(off)) =
                                        discr_entry.attr_value(gimli::DW_AT_data_member_location)
                                    {
                                        discr_member_offset = Some(off);
                                    }
                                }

                                // Further iterate through the children
                                let mut entries =
                                    unit.entries_at_offset(child_entry.offset()).unwrap();
                                entries.next_dfs().unwrap();

                                let variant_part_depth = entries.current().unwrap().depth();

                                // Keep track of the current variant
                                let mut variant: Option<EnumVariant> = None;

                                while let Some(sub_entry) = entries.next_dfs().unwrap() {
                                    if sub_entry.depth() <= variant_part_depth {
                                        break;
                                    }

                                    match sub_entry.tag() {
                                        gimli::DW_TAG_variant => {
                                            let mut discr_value = None;

                                            for attr in sub_entry.attrs() {
                                                match attr.name() {
                                                    gimli::DW_AT_discr_value => {
                                                        discr_value = match attr.value() {
                                                            gimli::AttributeValue::Data1(v) => {
                                                                Some(v as u8)
                                                            }
                                                            gimli::AttributeValue::Data2(v) => {
                                                                Some(v as u8)
                                                            }
                                                            gimli::AttributeValue::Data4(v) => {
                                                                Some(v as u8)
                                                            }
                                                            gimli::AttributeValue::Data8(v) => {
                                                                Some(v as u8)
                                                            }
                                                            gimli::AttributeValue::Sdata(v) => {
                                                                Some(v as u8)
                                                            }
                                                            _ => panic!(
                                                                "Discr value belongs to a type not parsed yet.\nValue is {:?}",
                                                                attr.value()
                                                            ),
                                                        }
                                                    }
                                                    _ => continue,
                                                }
                                            }

                                            if let Some(var) = variant.take() {
                                                enum_variants.push(var);
                                            }

                                            // Update varaint when encountering the variant tag
                                            // Only store variants with discr_value.
                                            // If it is missing it has been optimized and it will be in the fallback
                                            if let Some(discr_value) = discr_value {
                                                variant = Some(EnumVariant {
                                                    discr_value: Some(discr_value),
                                                    fields: Vec::new(),
                                                });
                                            }
                                        }
                                        gimli::DW_TAG_member => {
                                            let mut type_offset = None;
                                            let mut location = None;
                                            let mut name = None;

                                            for attr in sub_entry.attrs() {
                                                match attr.name() {
                                                    gimli::DW_AT_name => {
                                                        if let Ok(str) =
                                                            unit.attr_string(attr.value())
                                                        {
                                                            name = Some(
                                                                str.to_string_lossy()
                                                                    .unwrap()
                                                                    .to_string(),
                                                            );
                                                        }
                                                    }
                                                    gimli::DW_AT_type => {
                                                        if let gimli::AttributeValue::UnitRef(
                                                            offset,
                                                        ) = attr.value()
                                                        {
                                                            type_offset = Some(offset.0);
                                                        }
                                                    }
                                                    gimli::DW_AT_data_member_location => {
                                                        if let gimli::AttributeValue::Udata(loc) =
                                                            attr.value()
                                                        {
                                                            location = Some(loc);
                                                        }
                                                    }
                                                    _ => continue,
                                                }
                                            }

                                            if let (Some(name), Some(type_offset), Some(location)) =
                                                (name, type_offset, location)
                                            {
                                                let field = StructField {
                                                    name,
                                                    type_offset,
                                                    location,
                                                };

                                                if let Some(ref mut current_variant) = variant {
                                                    current_variant.fields.push(field);
                                                }
                                            }
                                        }
                                        _ => continue,
                                    }
                                }
                                if let Some(var) = variant {
                                    enum_variants.push(var);
                                }
                            }
                            _ => continue,
                        }
                    }
                }

                if let (Some(name), Some(byte_size), Some(alignment)) = (name, byte_size, alignment)
                {
                    if !is_enum {
                        let struct_type = DwarfType::Structure {
                            name,
                            byte_size,
                            alignment,
                            generics,
                            fields: fallback_fields,
                        };

                        let cache_node = TypeCacheNode {
                            dwarf_type: struct_type,
                            offset,
                        };

                        info_cache.type_index.insert(offset, cache_node);
                    } else {
                        // Add the fallback fields to a part of the main field
                        if !fallback_fields.is_empty() {
                            fallback_fields.retain(|fallback| {
                                !enum_variants.iter().any(|variant| {
                                    variant.fields.iter().any(|f| {
                                        f.type_offset == fallback.type_offset
                                            && f.location == fallback.location
                                    })
                                })
                            });
                            // Remove optimized fields, reserved for tuples to use
                            let filtered: Vec<StructField> = fallback_fields
                                .into_iter()
                                .filter(|var| !var.name.starts_with("__"))
                                .collect();

                            // Store the fallback fields
                            enum_variants.push(EnumVariant {
                                discr_value: None,
                                fields: filtered,
                            })
                        }

                        let enum_type = DwarfType::Variant {
                            name,
                            byte_size,
                            alignment,
                            discr_member_offset,
                            variants: enum_variants,
                        };

                        let cache_node = TypeCacheNode {
                            dwarf_type: enum_type,
                            offset,
                        };

                        info_cache.type_index.insert(offset, cache_node);
                    }
                }
                entries.next_dfs()?;
            }
            constants::DW_TAG_subprogram => {
                if let Some(function) = extract_subprogram(&mut entries, &unit) {
                    info_cache.execution_scopes.push(function);
                }
                entries.next_dfs()?;
            }
            constants::DW_TAG_inlined_subroutine => {
                if let Some(inline) = extract_inline(&mut entries, &unit) {
                    info_cache.execution_scopes.push(inline);
                }
                entries.next_dfs()?;
            }
            constants::DW_TAG_lexical_block => {
                if let Some(lexical_block) = extract_lexical_block(&mut entries, &unit) {
                    info_cache.execution_scopes.push(lexical_block);
                }
                entries.next_dfs()?;
            }
            _ => {
                entries.next_dfs()?;
            }
        }
    }
    Ok(())
}
