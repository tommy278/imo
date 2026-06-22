//! A simple example of parsing `.debug_info`.
//!
//! This example demonstrates how to parse the `.debug_info` section of a
//! DWARF object file and iterate over the compilation units and their DIEs.
//! It also demonstrates how to find the DWO unit for each CU in a DWP file.
//!
//! Most of the complexity is due to loading the sections from the object
//! file and DWP file, which is not something that is provided by gimli itself.

// style: allow verbose lifetimes
#![allow(clippy::needless_lifetimes)]

use gimli::{Reader as _, constants};
use object::{Object, ObjectSection};
use rustc_hash::FxHashMap;
use std::{borrow, error, fs};

#[derive(Debug)]
pub enum DwarfType {
    Base {
        name: String,
        encoding: u8,
        byte_size: u64,
    },
}

#[derive(Debug)]
pub struct TypeCacheNode {
    pub dwarf_type: DwarfType,
    pub offset: usize,
}

pub type TypeIndex = FxHashMap<usize, TypeCacheNode>;

// This is a simple wrapper around `object::read::RelocationMap` that implements
// `gimli::read::Relocate` for use with `gimli::RelocateReader`.
// You only need this if you are parsing relocatable object files.
#[derive(Debug, Default)]
struct RelocationMap(object::read::RelocationMap);

impl<'a> gimli::read::Relocate for &'a RelocationMap {
    fn relocate_address(&self, offset: usize, value: u64) -> gimli::Result<u64> {
        Ok(self.0.relocate(offset as u64, value))
    }

    fn relocate_offset(&self, offset: usize, value: usize) -> gimli::Result<usize> {
        <usize as gimli::ReaderOffset>::from_u64(self.0.relocate(offset as u64, value as u64))
    }
}

// The section data that will be stored in `DwarfSections` and `DwarfPackageSections`.
#[derive(Default)]
struct Section<'data> {
    data: borrow::Cow<'data, [u8]>,
    relocations: RelocationMap,
}

// The reader type that will be stored in `Dwarf` and `DwarfPackage`.
// If you don't need relocations, you can use `gimli::EndianSlice` directly.
type Reader<'data> =
    gimli::RelocateReader<gimli::EndianSlice<'data, gimli::RunTimeEndian>, &'data RelocationMap>;

pub fn lookup_vars(_binary_path: &str) {
    // TODO: Change this to be dynamic
    let file =
        fs::File::open("/Users/tommy/Projects/imo/src/test/linux/multiple/inline_test").unwrap();
    // SAFETY: This is not safe. `gimli` does not mitigate against modifications to the
    // file while it is being read. See the `memmap2` documentation and take your own
    // precautions. `fs::read` could be used instead if you don't mind loading the entire
    // file into memory.
    let mmap = unsafe { memmap2::Mmap::map(&file).unwrap() };
    let object = object::File::parse(&*mmap).unwrap();
    let endian = if object.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };
    dump_file(&object, endian).unwrap();
}

fn dump_file(
    object: &object::File,
    endian: gimli::RunTimeEndian,
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
    let dwarf = dwarf_sections.borrow(|section| borrow_section(section, endian));
    // Iterate over the compilation units.
    let mut iter = dwarf.units();
    while let Some(header) = iter.next()? {
        // println!("Unit at <.debug_info+0x{:x}>", header.offset().0);
        let unit = dwarf.unit(header)?;
        let unit_ref = unit.unit_ref(&dwarf);
        dump_unit(unit_ref)?;
    }

    Ok(())
}

fn dump_unit(unit: gimli::UnitRef<Reader>) -> Result<(), gimli::Error> {
    // Iterate over the Debugging Information Entries (DIEs) in the unit.
    let mut entries = unit.entries();
    let mut tmp_storage = Vec::new();

    while let Some(entry) = entries.next_dfs()? {
        /* println!(
            "<{}><{:x}> {}",
            entry.depth(),
            entry.offset().0,
            entry.tag()
        ); */

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
                    let dwarf_type = DwarfType::Base {
                        name,
                        encoding,
                        byte_size,
                    };
                    let cache_node = TypeCacheNode {
                        dwarf_type,
                        offset: entry.offset().0,
                    };
                    tmp_storage.push(cache_node);
                }
            }
            _ => {
                // TODO: Parse more types
                continue;
            }
        }

        tmp_storage.iter().for_each(|data| {
            println!("{:?}", data);
        });

        // Iterate over the attributes in the DIE.
        /* for attr in entry.attrs() {
            print!("   {}: {:?}", attr.name(), attr.value());
            if let Ok(s) = unit.attr_string(attr.value()) {
                print!(" '{}'", s.to_string_lossy()?);
            }
            println!();
        } */
    }
    Ok(())
}
