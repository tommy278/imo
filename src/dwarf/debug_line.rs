use crate::session::*;
use crate::types::{LineRow, SourceLocation, StringId, UniqueFileId};
use object::{Object, ObjectSection};
use rustc_hash::FxHashSet;
use std::{
    borrow,
    path::{self},
};

use thiserror;

/*
 * Based on the 'simple_line' example from the gimli project.
 * Source: https://github.com/gimli-rs/gimli/blob/main/crates/examples/src/bin/simple_line.rs
 * * The implementation below was adapted to support specific address lookup
 * and integrated into the project's native debugger architecture.
 */

#[derive(Debug, thiserror::Error)]
pub enum DebugLineError {
    #[error("Failed to load debug line section")]
    LoadingSection(#[from] object::Error),

    #[error("Failed to parse DWARF debug line")]
    ParsingSection(#[from] gimli::Error),
}

/// Get details from dwarf and apply them to the debug session cache
pub fn setup_session_cache(
    object: &object::File<'_>,
    session: &mut DebugSession,
) -> Result<(), DebugLineError> {
    let endian = if object.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };
    update_session_cache(&object, endian, session)?;
    Ok(())
}

/// Update the BreakpointTarget and SourceLocation in the debug session cache
fn update_session_cache(
    object: &object::File,
    endian: gimli::RunTimeEndian,
    session: &mut DebugSession,
) -> Result<(), DebugLineError> {
    // Load a section and return as `Cow<[u8]>`.
    let load_section = |id: gimli::SectionId| -> Result<borrow::Cow<[u8]>, DebugLineError> {
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
            let mut current_file_id: Option<StringId> = None;

            // Track the last indexed file to avoid instruction duplication per line
            let mut last_indexed_file: Option<StringId> = None;
            let mut registered_lines: FxHashSet<u32> = FxHashSet::default();

            let mut active_range_start: Option<(u64, SourceLocation)> = None;

            // Iterate over the line program rows.
            let mut rows = program.rows();
            while let Some((header, row)) = rows.next_row()? {
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
                if current_file_id.is_none() || path != current_raw_path {
                    current_raw_path = path.clone();
                    current_file_id =
                        Some(session.interner.get_or_intern(path.to_str().unwrap_or("")));
                }

                // Determine line/column. DWARF line/column is never 0, so we use that
                // but other applications may want to display this differently.
                let line = match row.line() {
                    Some(line) => line.get() as u32,
                    None => 0,
                };

                // This unwrap is safe because we guranteed it has a value above
                let file_id = current_file_id.unwrap();

                session.file_indices.insert(
                    UniqueFileId {
                        offset: unit.offset().0,
                        file_idx: row.file_index(),
                    },
                    file_id,
                );

                // DEDUPLICATION LOGIC
                // Only push to line index if this row starts a new line or swicthed to a
                // different file

                if last_indexed_file.map_or(true, |f| f != file_id) {
                    registered_lines.clear();
                    last_indexed_file = Some(file_id);
                }

                // Ignore line 0 for breakpoint indexing
                if line == 0 {
                    continue;
                }

                let is_already_registered = registered_lines.contains(&line);
                let relative_address = row.address();

                if let Some((start_addr, location)) = active_range_start {
                    if relative_address > start_addr && location.line != 0 {
                        session.line_row.push(LineRow {
                            location,
                            start_address: start_addr,
                            end_address: relative_address,
                            is_stmt: row.is_stmt(),
                        });
                    }
                }

                if row.end_sequence() {
                    active_range_start = None;
                } else {
                    active_range_start = Some((
                        relative_address,
                        SourceLocation {
                            file: file_id,
                            line: line,
                        },
                    ));
                }

                if !is_already_registered && !row.end_sequence() {
                    session
                        .line_index
                        .entry(line)
                        .or_insert_with(Vec::new)
                        .push(breakpoint::BreakpointTarget {
                            file: path.into_boxed_path(),
                            relative_address,
                        });

                    let declaration_history = session
                        .file_declaration_order
                        .entry(file_id)
                        .or_insert_with(Vec::new);

                    // Only push if not on the same line
                    if declaration_history.last() != Some(&line) {
                        declaration_history.push(line);
                    }

                    registered_lines.insert(line);
                }
            }
        }
    }

    Ok(())
}
