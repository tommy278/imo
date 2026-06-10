use object::{Object, ObjectSection};
use rustc_hash::FxHashMap;
use std::{
    borrow, error,
    fs::{self, read_to_string},
    path::{self, Path},
    rc::Rc,
};

#[derive(Default)]
pub struct SourceLocation {
    pub file: Rc<Path>,
    pub line: u64,
}

#[derive(Default)]
pub struct BreakpointTarget {
    pub file: Rc<Path>,
    pub relative_address: u64,
}

#[derive(Default)]
pub struct DebugSession {
    pub base_address: u64,
    pub line_index: FxHashMap<u64, Vec<BreakpointTarget>>,
    pub address_to_location: FxHashMap<u64, SourceLocation>,
}

impl DebugSession {
    fn init(pid: nix::unistd::Pid) -> Self {
        let mut session = Self::default();

        session.update_process_base_address(pid);

        session
    }

    pub fn update_process_base_address(&mut self, pid: nix::unistd::Pid) {
        let mut base_address = 0;
        let maps_path = format!("/proc/{}/maps", pid);
        if let Ok(content) = read_to_string(maps_path) {
            if let Some(first_line) = content.lines().next() {
                if let Some(base_str) = first_line.split('-').next() {
                    base_address = u64::from_str_radix(base_str, 16).unwrap_or(0);
                }
            }
        }
        self.base_address = base_address;
    }
}

/*
 * Based on the 'simple_line' example from the gimli project.
 * Source: https://github.com/gimli-rs/gimli/blob/main/crates/examples/src/bin/simple_line.rs
 * * The implementation below was adapted to support specific address lookup
 * and integrated into the project's native debugger architecture.
 */

pub fn lookup_address_by_line(binary_path: &str, session: &mut DebugSession) -> Option<u64> {
    let file = fs::File::open(&binary_path).unwrap();
    // TODO: Find a safer way to map file as suggested by gimli
    let mmap = unsafe { memmap2::Mmap::map(&file).unwrap() };
    let object = object::File::parse(&*mmap).unwrap();
    let endian = if object.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };
    dump_file(&object, endian, session).unwrap()
}

fn dump_file(
    object: &object::File,
    endian: gimli::RunTimeEndian,
    session: &mut DebugSession,
) -> Result<Option<u64>, Box<dyn error::Error>> {
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

                    // Determine line/column. DWARF line/column is never 0, so we use that
                    // but other applications may want to display this differently.
                    let line = match row.line() {
                        Some(line) => line.get(),
                        None => 0,
                    };

                    // TODO: Decide what to do with column
                    /* let column = match row.column() {
                        gimli::ColumnType::LeftEdge => 0,
                        gimli::ColumnType::Column(column) => column.get(),
                    }; */

                    if path.ends_with(target_file) && line == target_line {
                        let target_address = row.address();
                        return Ok(Some(target_address));
                    }
                }
            }
        }
    }
    Ok(None)
}
