use crate::session::*;
use object::{Object, ObjectSection};
use std::{
    borrow, error,
    fs::{self},
    path::{self, Path},
    rc::Rc,
};

/*
 * Based on the 'simple_line' example from the gimli project.
 * Source: https://github.com/gimli-rs/gimli/blob/main/crates/examples/src/bin/simple_line.rs
 * * The implementation below was adapted to support specific address lookup
 * and integrated into the project's native debugger architecture.
 */

/// Get details from dwarf and apply them to the debug session cache
pub fn setup_session_cache(binary_path: &str, session: &mut DebugSession) {
    let file = fs::File::open(&binary_path).unwrap();
    // TODO: Find a safer way to map file as suggested by gimli
    let mmap = unsafe { memmap2::Mmap::map(&file).unwrap() };
    let object = object::File::parse(&*mmap).unwrap();
    let endian = if object.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };
    update_session_cache(&object, endian, session).unwrap();
}

/// Update the BreakpointTarget and SourceLocation in the debug session cache
fn update_session_cache(
    object: &object::File,
    endian: gimli::RunTimeEndian,
    session: &mut DebugSession,
) -> Result<(), Box<dyn error::Error>> {
    // Load a section and return as `Cow<[u8]>`.
    let load_section = |id: gimli::SectionId| -> Result<borrow::Cow<[u8]>, Box<dyn error::Error>> {
        Ok(match object.section_by_name(id.name()) {
            Some(section) => section.uncompressed_data()?,
            None => borrow::Cow::Borrowed(&[]),
        })
    };

    // Borrow a `Cow<[u8]>` to create an `EndianSlice`.
    let borrow_section = |section| gimli::EndianSlice::new(borrow::Cow::as_ref(section), endian);

    // Load all of the sections.
    let dwarf_sections = gimli::DwarfSections::load(&load_section)?;

    // Create `EndianSlice`s for all of the sections.
    let dwarf = dwarf_sections.borrow(borrow_section);

    // Iterate over the compilation units.
    let mut iter = dwarf.units();
    while let Some(header) = iter.next()? {
        let unit = dwarf.unit(header)?;
        let unit = unit.unit_ref(&dwarf);

        // Get the line program for the compilation unit.
        if let Some(program) = unit.line_program.clone() {
            let comp_dir = if let Some(ref dir) = unit.comp_dir {
                path::PathBuf::from(dir.to_string_lossy().into_owned())
            } else {
                path::PathBuf::new()
            };

            // Track the active path and its Rc allocations across iterations
            let mut current_raw_path = path::PathBuf::new();
            let mut current_file_rc: Option<Rc<Path>> = None;

            // Iterate over the line program rows.
            let mut rows = program.rows();
            while let Some((header, row)) = rows.next_row()? {
                if !row.end_sequence() {
                    // Determine the path. Real applications should cache this for performance.
                    let mut path = path::PathBuf::new();
                    if let Some(file) = row.file(header) {
                        path.clone_from(&comp_dir);

                        // The directory index 0 is defined to correspond to the compilation unit directory.
                        if file.directory_index() != 0 {
                            if let Some(dir) = file.directory(header) {
                                path.push(unit.attr_string(dir)?.to_string_lossy().as_ref());
                            }
                        }

                        path.push(
                            unit.attr_string(file.path_name())?
                                .to_string_lossy()
                                .as_ref(),
                        );
                    }

                    // Only perform a heap allocation when the file path changes
                    if current_file_rc.is_none() || path != current_raw_path {
                        current_raw_path = path.clone();
                        current_file_rc = Some(Rc::from(path.as_path()));
                    }

                    // Determine line/column. DWARF line/column is never 0, so we use that
                    // but other applications may want to display this differently.
                    let line = match row.line() {
                        Some(line) => line.get(),
                        None => 0,
                    };

                    // This unwrap is safe because we guranteed it has a value above
                    let file_rc = current_file_rc.as_ref().unwrap();

                    session
                        .line_index
                        .entry(line)
                        .or_insert_with(Vec::new)
                        .push(BreakpointTarget {
                            file: Rc::clone(file_rc),
                            relative_address: row.address(),
                        });

                    let absolute_address = session.base_address + row.address();

                    session.address_to_location.insert(
                        absolute_address,
                        SourceLocation {
                            file: Rc::clone(file_rc),
                            line,
                        },
                    );
                }
            }
        }
    }
    Ok(())
}

