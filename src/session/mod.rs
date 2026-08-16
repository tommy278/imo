pub mod interface;
#[cfg(target_os = "linux")]
pub mod linux;

pub mod error;

use gimli::UnwindSection;
use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::helpers::dwarf::{
    self,
    debug_frame::{RawDebugFrame, setup_session_debug_frame},
    debug_info::{ActiveVariablesContext, DebuggerMetadataCache},
    error::CacheSetupError,
};
use crate::interface::{DebugValue, RegisterViewer};
use crate::session::error::SystemError;
use crate::session::interface::{BreakpointData, BreakpointMutationResult, BreakpointTarget};

#[cfg(target_os = "linux")]
pub use crate::session::linux as os;

// If not supported yet, add dummy values for compilation
#[cfg(not(target_os = "linux"))]
pub mod os {
    // Dummy types to satisfy the type aliases
    pub type ProcessId = i32;
    pub type PlatformRegStruct = ();

    use crate::{helpers::dwarf::error::CacheSetupError, session::error::SystemError};

    #[derive(Debug, Clone)]
    pub struct PlatformBreakpoint;

    impl PlatformBreakpoint {
        pub fn new(_absolute_address: u64) -> Self {
            unimplemented!("imo debugger only runs on linux")
        }
        pub fn enable(&self, _pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on linux")
        }
        pub fn disable(&self, _pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on linux")
        }
    }

    pub fn send_trap_signal(_pid: ProcessId) -> Result<(), SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn get_process_base_address(_pid: ProcessId) -> Result<u64, CacheSetupError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn read_bytes(_pid: ProcessId, _ptr: usize, _len: usize) -> Result<Vec<u8>, SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn step(_pid: ProcessId) -> Result<(), SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn continue_session(_pid: ProcessId) -> Result<(), SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn kill_session(_pid: ProcessId) -> Result<(), SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn peek_data(_pid: ProcessId, _address: u64) -> Result<i64, SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn get_regs(_pid: ProcessId) -> Result<PlatformRegStruct, SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }
}

pub type ProcessId = os::ProcessId;
pub type PlatformRegStruct = os::PlatformRegStruct;

#[derive(Debug)]
pub struct ManagedBreakpoint {
    pub breakpoint: os::PlatformBreakpoint,
    pub ref_count: usize,
}

impl ManagedBreakpoint {
    pub fn new(breakpoint: os::PlatformBreakpoint) -> Self {
        Self {
            breakpoint,
            ref_count: 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SourceLocation {
    pub file: StringId,
    pub line: u32,
}

#[derive(Debug, Default)]
pub struct SourceCodeInfo {
    source_code: String,
    line_offsets: Vec<usize>,
}

pub enum SourceCodeDisplay<'a> {
    FullyResolved {
        source_code: &'a str,
        line_number: u32,
    },
    PartiallyResolved {
        path: Box<Path>,
        line_number: u32,
    },
    CacheCorrupt {
        id: StringId,
    },
    Unresolved,
}

/// Display the code in a user friendly format
pub fn display_source_code(f: &mut std::fmt::Formatter<'_>, code: &str) -> std::fmt::Result {
    for token in code.split_whitespace() {
        match token {
            "let" | "fn" | "pub" | "impl" => write!(f, "{} ", token.red())?,
            "mut" | "return" | "if" => write!(f, "{} ", token.purple())?,
            "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "u64" | "i64" | "usize" | "isize"
            | "f32" | "f64" | "bool" => write!(f, "{} ", token.bright_yellow())?,
            _ => write!(f, "{token} ")?,
        }
    }
    Ok(())
}

impl std::fmt::Display for SourceCodeDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FullyResolved {
                source_code,
                line_number,
            } => {
                write!(f, "{}\t", line_number.cyan())?;
                display_source_code(f, *source_code)?;
            }
            Self::PartiallyResolved { path, line_number } => write!(
                f,
                "Could not locate {}. Line number: {}.",
                path.display().bright_blue(),
                line_number.cyan()
            )?,
            Self::Unresolved => write!(f, "Could not resolve current location")?,
            Self::CacheCorrupt { id } => write!(
                f,
                "Could not resolve string with id {:?}. Restarting session might fix this issue",
                id
            )?,
        }
        Ok(())
    }
}

#[derive(Default, Debug)]
pub struct SourceCodeCache {
    pub entries: FxHashMap<PathBuf, SourceCodeInfo>,
}

impl SourceCodeCache {
    pub fn get_line_entry(&self, path: &Path, line: u32) -> Option<&str> {
        if let Some(entry) = self.entries.get(path) {
            let index = line.saturating_sub(1) as usize;
            let line_offset = entry.line_offsets.get(index)?;
            let next_line_offset = entry.line_offsets.get(index + 1)?;

            return Some(&entry.source_code[*line_offset..*next_line_offset]);
        }

        None
    }

    pub fn create_and_get_line_entry(&mut self, path: &Path, line: u32) -> Option<&str> {
        if !path.exists() {
            return None;
        }

        let file = fs::File::open(path).unwrap();
        let reader = BufReader::new(file);

        let mut source_code: String = String::new();
        let mut line_offsets: Vec<usize> = Vec::new();

        for line in reader.lines() {
            line_offsets.push(source_code.len());
            source_code.push_str(line.ok()?.as_ref());
        }

        let source_code_info = SourceCodeInfo {
            source_code,
            line_offsets,
        };

        self.entries.insert(path.into(), source_code_info);
        self.get_line_entry(path, line)
    }
}

// The command to be ran when the debugger hits a sigtrap
#[derive(Default, Debug)]
pub enum CurrentStopCmd {
    SingleStep,
    StepOver {
        start_stack_pointer: u64,
        start_file: StringId,
        start_line: u32,
        started_from_inline: bool,
    },
    StepInto {
        start_file: StringId,
        start_line: u32,
    },
    StepOut {
        original_stack_pointer: u64,
        original_file: StringId,
        original_line: u32,
        return_address: u64,
        started_from_inline: bool,
    },
    FinishStepOver {
        original_stack_pointer: u64,
        original_file: StringId,
        original_line: u32,
        started_from_inline: bool,
    },
    Finish {
        start_stack_pointer: u64,
        started_from_inline: bool,
    },
    StepOutFinish {
        original_stack_pointer: u64,
        return_address: u64,
        started_from_inline: bool,
    },
    CompleteFinish {
        start_stack_pointer: u64,
        started_from_inline: bool,
    },
    #[default]
    Idle,
    Running,
    Continuing,
    SearchingForValidLocation,
    Completed,
}

impl CurrentStopCmd {
    pub fn is_completed(&self) -> bool {
        match self {
            CurrentStopCmd::Completed => true,
            _ => false,
        }
    }

    pub fn is_idle(&self) -> bool {
        match self {
            CurrentStopCmd::Idle => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub struct StringId(u16);

#[derive(Debug, Clone, Default)]
pub struct StringInterner {
    buffer: Vec<String>,
    map: FxHashMap<String, StringId>,
}

impl StringInterner {
    pub fn get_or_intern(&mut self, str: String) -> StringId {
        if let Some(id) = self.map.get(&str) {
            return *id;
        }

        let new_id = StringId(self.buffer.len() as u16);
        self.buffer.push(str.clone());
        self.map.insert(str, new_id);

        new_id
    }

    pub fn get_string(&self, id: StringId) -> Option<String> {
        self.buffer.get(id.0 as usize).cloned()
    }
}

#[derive(Debug)]
pub struct LineRow {
    pub location: SourceLocation,
    pub start_address: u64,
    pub end_address: u64,
    pub is_stmt: bool,
}

/// Cache for entire debug session
#[derive(Debug)]
pub struct DebugSession {
    // Breakpoint data
    pub line_index: FxHashMap<u32, Vec<BreakpointTarget>>,
    pub base_address: u64,
    pub breakpoint_index_tracker: Vec<Option<BreakpointData>>,

    pub line_row: Vec<LineRow>,

    // Used to find out the actual order in while files were declared
    pub file_declaration_order: FxHashMap<StringId, Vec<u32>>,

    // A global arena to consolidate repetitive string allocations into one location
    pub interner: StringInterner,

    pub source_file: SourceCodeCache,

    // Metdata
    pub metadata: DebuggerMetadataCache,

    // Tracking the current state the debugger is in
    pub current_cmd: CurrentStopCmd,

    // Debug frame for finding CFA
    pub raw_debug_frame: RawDebugFrame,

    // Current register states
    pub registers: Option<RegisterViewer>,

    // Different for each os
    pub active_breakpoints: FxHashMap<u64, ManagedBreakpoint>,
    pub pid: os::ProcessId,
}

impl DebugSession {
    /// Instantiate the struct with default values
    fn from_pid(pid: os::ProcessId) -> Self {
        Self {
            base_address: 0,
            breakpoint_index_tracker: Vec::new(),
            line_index: FxHashMap::default(),
            file_declaration_order: FxHashMap::default(),
            metadata: DebuggerMetadataCache::default(),
            raw_debug_frame: RawDebugFrame::default(),
            current_cmd: CurrentStopCmd::default(),
            active_breakpoints: FxHashMap::default(),
            interner: StringInterner::default(),
            source_file: SourceCodeCache::default(),
            line_row: Vec::new(),
            registers: None,
            pid,
        }
    }

    // =================================================================
    // OS Specific functions
    // =================================================================

    /// Create a complete instance of the session cache
    pub fn new(
        pid: os::ProcessId,
        binary_path: &str,
    ) -> Result<Self, dwarf::error::CacheSetupError> {
        let mut session = Self::from_pid(pid);

        let file = std::fs::File::open(binary_path)?;

        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        let object = object::File::parse(&*mmap)?;

        session.update_process_base_address()?;

        session.metadata = DebuggerMetadataCache::new(&object)?;

        // Update line index and address to location
        dwarf::debug_line::setup_session_cache(&object, &mut session)?;

        session.raw_debug_frame = setup_session_debug_frame(&object)?;

        session.set_up_line_row();

        Ok(session)
    }

    /// Remove unnecessary ranges and sort the address for binary search lookup
    pub fn set_up_line_row(&mut self) {
        self.line_row
            .retain(|range| range.start_address >= self.metadata.text_address);

        self.line_row.sort_by_key(|l| l.start_address);
    }

    pub fn find_line_range(&self, current_pc: u64) -> Option<&LineRow> {
        match self.line_row.binary_search_by(|p| {
            if current_pc < p.start_address {
                std::cmp::Ordering::Greater
            } else if current_pc >= p.end_address {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            Ok(index) => Some(&self.line_row[index]),
            Err(_) => None,
        }
    }

    /// Get the live register of the current process
    pub fn get_regs(&self) -> Result<RegisterViewer, error::SystemError> {
        if let Some(regs) = self.registers {
            return Ok(regs);
        }

        // Backup in case the register was not instantiated for some reason
        let regs = os::get_regs(self.pid)?;
        Ok(RegisterViewer { regs })
    }

    pub fn invalidate_register(&mut self) {
        self.registers = None;
    }

    /// Create a specific breakpoint at a given address
    pub fn create_specific_breakpoint(&mut self, relative_address: u64) -> Result<(), SystemError> {
        let absolute_address = self.get_absolute_address(relative_address);

        // If breakpoint already exists dont write simply increment the reference counter
        if let Some(managed_breakpoint) = self.active_breakpoints.get_mut(&absolute_address) {
            managed_breakpoint.ref_count += 1;
            return Ok(());
        }

        // First time seeing the address
        // Create the breakpoint
        let mut breakpoint = os::PlatformBreakpoint::new(absolute_address);
        breakpoint.enable(self.pid)?;

        self.active_breakpoints
            .insert(absolute_address, ManagedBreakpoint::new(breakpoint));

        Ok(())
    }

    pub fn get_unwind_table(&self) -> gimli::EhFrame<gimli::EndianSlice<'_, gimli::RunTimeEndian>> {
        self.raw_debug_frame
            .get_unwind_table_with_endian(self.metadata.endian)
    }

    pub fn get_register_value(&self, register: gimli::Register) -> Option<u64> {
        let regs = self.get_regs().ok()?.regs;

        #[cfg(target_os = "linux")]
        {
            match register.0 {
                6 => Some(regs.rbp),
                7 => Some(regs.rsp),
                16 => Some(regs.rip),
                _ => todo!("Not implemented yet {}", register.0),
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            Some(0)
        }
    }

    pub fn get_return_address(&self) -> Option<u64> {
        let eh_frame = self.get_unwind_table();
        let base_addresses = self.metadata.base_addresses.clone();

        let current_pc = self.current_pc().ok()?;

        if let Ok(fde) =
            eh_frame.fde_for_address(&base_addresses, current_pc, |sections, bases, offset| {
                sections.cie_from_offset(bases, offset)
            })
        {
            let mut ctx = gimli::UnwindContext::new();
            let mut table = fde.rows(&eh_frame, &base_addresses, &mut ctx).ok()?;

            let ra_register = fde.cie().return_address_register();
            while let Some(row) = table.next_row().ok()? {
                let cfa_address = match row.cfa() {
                    gimli::CfaRule::RegisterAndOffset { register, offset } => {
                        let reg_value = self.get_register_value(*register)?;
                        (reg_value as i64 + offset) as u64
                    }
                    gimli::CfaRule::Expression(_) => todo!(),
                };

                if row.contains(current_pc) {
                    if let Some(ra_rule) = row.register(ra_register) {
                        match ra_rule {
                            gimli::RegisterRule::Offset(offset) => {
                                let ra_storage_address = (cfa_address as i64 + offset) as u64;
                                let return_address =
                                    os::peek_data(self.pid, ra_storage_address).ok()? as u64;
                                return Some(return_address);
                            }
                            gimli::RegisterRule::Register(saved_reg) => {
                                let return_address = self.get_register_value(saved_reg)?;
                                return Some(return_address);
                            }
                            _ => todo!(),
                        }
                    }
                }
            }
        }
        None
    }

    pub fn current_instruction_pointer(&self) -> Result<u64, SystemError> {
        self.get_regs().map(|r| r.instruction_pointer())
    }

    pub fn current_stack_pointer(&self) -> Result<u64, SystemError> {
        self.get_regs().map(|r| r.stack_pointer())
    }

    pub fn current_pc(&self) -> Result<u64, SystemError> {
        let pc = self.current_instruction_pointer()? - self.base_address;
        Ok(pc)
    }

    /// Continue session from last interrupt
    pub fn continue_session(&self) -> Result<(), error::SystemError> {
        os::continue_session(self.pid)
    }

    pub fn send_trap_signal(&self) -> Result<(), error::SystemError> {
        os::send_trap_signal(self.pid)
    }

    // ========================================
    // CLI Commands
    // ========================================

    pub fn is_idle(&self) -> bool {
        self.current_cmd.is_idle()
    }

    pub fn toggle_running(&mut self) {
        self.current_cmd = CurrentStopCmd::Running;
    }
    pub fn toggle_continue(&mut self) {
        self.current_cmd = CurrentStopCmd::Continuing;
    }

    pub fn complete_single_step(&mut self) -> Result<(), SystemError> {
        self.invalidate_register();
        self.current_cmd = CurrentStopCmd::SingleStep;
        self.single_step()
    }

    pub fn begin_step_into(&mut self) -> Result<(), SystemError> {
        self.invalidate_register();
        let Some(current_location) = self.current_location() else {
            self.current_cmd = CurrentStopCmd::SearchingForValidLocation;
            self.single_step()?;
            return Ok(());
        };

        self.current_cmd = CurrentStopCmd::StepInto {
            start_file: current_location.file.clone(),
            start_line: current_location.line,
        };
        self.single_step()
    }

    pub fn begin_step_over(&mut self) -> Result<(), SystemError> {
        self.invalidate_register();
        let Some(current_location) = self.current_location() else {
            self.current_cmd = CurrentStopCmd::SearchingForValidLocation;
            self.single_step()?;
            return Ok(());
        };

        let is_inline = self.metadata.is_in_inline(self.current_pc()?);

        self.current_cmd = CurrentStopCmd::StepOver {
            start_stack_pointer: self.current_stack_pointer()?,
            start_file: current_location.file,
            start_line: current_location.line,
            started_from_inline: is_inline,
        };
        self.single_step()
    }

    pub fn begin_finish(&mut self) -> Result<(), SystemError> {
        self.invalidate_register();
        let is_inline = self.metadata.is_in_inline(self.current_pc()?);

        self.current_cmd = CurrentStopCmd::Finish {
            start_stack_pointer: self.current_stack_pointer()?,
            started_from_inline: is_inline,
        };
        self.single_step()
    }

    pub fn current_location(&self) -> Option<&SourceLocation> {
        let abs = self.current_instruction_pointer().ok()?;
        let rel_addr = self.get_relative_address(abs);
        self.get_location_with_address(rel_addr)
    }

    pub fn get_or_create_source_file(&mut self, path: &Path, line: u32) -> Option<&str> {
        if let Some(entry) = self.source_file.get_line_entry(path, line) {
            // Returning normally confuses compiler
            // NOTE: This is safe because entry is guaranteed to exist simply casting it as a raw pointer
            unsafe { return Some(&*(entry as *const str)) }
        }
        self.source_file.create_and_get_line_entry(path, line)
    }

    pub fn get_current_source_file(&mut self) -> SourceCodeDisplay<'_> {
        let Some(current_location) = self.current_location().copied() else {
            return SourceCodeDisplay::Unresolved;
        };

        let Some(path_string) = self.interner.get_string(current_location.file) else {
            return SourceCodeDisplay::CacheCorrupt {
                id: current_location.file,
            };
        };

        let path = Path::new(&path_string);
        let line_number = current_location.line;

        if let Some(source_code) = self.get_or_create_source_file(path, current_location.line) {
            return SourceCodeDisplay::FullyResolved {
                source_code,
                line_number,
            };
        }

        SourceCodeDisplay::PartiallyResolved {
            path: Box::from(path),
            line_number,
        }
    }

    /// Move forward from the specified stop
    pub fn single_step(&self) -> Result<(), error::SystemError> {
        os::step(self.pid)
    }

    /// Kill the current session
    pub fn kill_session(&self) -> Result<(), error::SystemError> {
        os::kill_session(self.pid)
    }

    /// Get and update the process base address
    pub fn update_process_base_address(&mut self) -> Result<(), CacheSetupError> {
        self.base_address = os::get_process_base_address(self.pid)?;
        Ok(())
    }

    // =================================================================
    // Other Methods
    // =================================================================

    /// Obtain the location that an address belongs to within the program
    pub fn get_location_with_address(&self, relative_address: u64) -> Option<&SourceLocation> {
        self.find_line_range(relative_address).map(|l| &l.location)
    }

    /// Get the exact order in which the compiler actually initialized the variables
    /// Rust does not always initialize variables sequentially
    pub fn get_file_decl_order(&self, file: StringId) -> Option<&Vec<u32>> {
        self.file_declaration_order.get(&file)
    }

    /// Get the relative address from absolute address
    pub fn get_relative_address(&self, absolute_address: u64) -> u64 {
        absolute_address - self.base_address
    }

    /// Find current scope with internal pc
    pub fn find_current_scope(&self) -> Result<ActiveVariablesContext<'_>, SystemError> {
        let current_pc = self.current_instruction_pointer()? - self.base_address;
        let context = self.metadata.find_scope_by_pc(current_pc);
        Ok(context)
    }

    /// Get the value of a variable with the given name
    /// Requires current scope to evaluate the value
    pub fn get_var_value(
        &self,
        node: &ActiveVariablesContext,
        name: &str,
    ) -> Result<Option<DebugValue>, error::VariableParseError> {
        let regs = self.get_regs()?;

        let endian = self.metadata.endian;
        let abi = &self.metadata.abi;
        let Some(encoding) = self.metadata.encoding else {
            return Err(error::VariableParseError::Encoding);
        };

        // Since rust does not declare variables in sequential order
        // We store the actual order the variables were actually declared
        // We use this to check if the decl_line is before our current index
        // If it is that means it has been declared and if not then it hasnt

        if let Some(variable) = node.get_variable_with_name(name) {
            let current_pc = regs.instruction_pointer() - self.base_address;

            if let Some(info) = self.get_location_with_address(current_pc) {
                let SourceLocation { file, line } = info;

                if let Some(line_order) = self.get_file_decl_order(*file) {
                    if let Some(current_idx) = line_order.iter().position(|&l| l == *line) {
                        if let Some(var_decl_idx) =
                            line_order.iter().position(|&l| l == variable.decl_line)
                        {
                            if var_decl_idx >= current_idx {
                                return Ok(Some(DebugValue::Err(
                                    "Variable not initialized yet".to_string(),
                                )));
                            }
                        }
                    }
                }
            } else {
                return Ok(None);
            }

            let Some(frame_base) = node.frame_base else {
                return Err(error::VariableParseError::FrameBase);
            };

            // Get the variable's address
            let Some(address) = variable.parse_value(
                &regs,
                encoding,
                endian,
                abi,
                frame_base,
                &self.metadata.type_index,
                self.pid,
            )?
            else {
                return Err(error::VariableParseError::Address);
            };

            // Resolve the variable's live value with address and current pid
            if let Some(ty) = self.metadata.type_index.get(&variable.target_type_offset) {
                let result =
                    ty.dwarf_type
                        .to_debug_value(&self.metadata.type_index, address, self.pid)?;

                return Ok(result);
            }
        }
        Ok(None)
    }

    /// Clear all breakpoint for line_number by default
    /// Only clear specified breakpoints if file name is provided
    pub fn clear_breakpoint(
        &mut self,
        line_number: u32,
        file: Option<&str>,
    ) -> Result<Vec<usize>, SystemError> {
        let mut cleared_breakpoints = Vec::new();
        let mut bp_idx = Vec::new();

        let filter_by_file = file.is_some();

        // Map every breakpoints that macthes the user's choice into being None
        // Store these breakpoints and their indices
        for (idx, opt_bp) in self.breakpoint_index_tracker.iter_mut().enumerate() {
            if let Some(bp) = opt_bp {
                if !filter_by_file {
                    if bp.line == line_number {
                        if let Some(removed_bp) = opt_bp.take() {
                            cleared_breakpoints.push(removed_bp);
                            bp_idx.push(idx + 1);
                        }
                    }
                } else {
                    if let Some(bp_file) = bp.file.to_str() {
                        // Safe unwrap since this is the path where the file is Some
                        if bp.line == line_number && bp_file.ends_with(file.unwrap()) {
                            if let Some(removed_bp) = opt_bp.take() {
                                cleared_breakpoints.push(removed_bp);
                                bp_idx.push(idx + 1);
                            }
                        }
                    } else {
                        eprintln!("[Warning] Failed to convert file path at index {}", idx)
                    }
                }
            }
        }

        for data in cleared_breakpoints.iter() {
            for bp in data.target.iter() {
                self.clear_specific_breakpoint(bp.relative_address)?
            }
        }

        Ok(bp_idx)
    }

    /// Enable breakpoint at a specific index in the tracker
    pub fn enable_breakpoint(
        &mut self,
        index: usize,
    ) -> Result<BreakpointMutationResult, SystemError> {
        // NOTE: Safe index, bounds are checked by the cli
        let target = self.breakpoint_index_tracker[index].clone();

        if let Some(mut data) = target {
            // If already enabled, DO NOTHING
            if data.enabled {
                return Ok(BreakpointMutationResult::AlreadyInState);
            }

            for bp in data.target.iter() {
                self.create_specific_breakpoint(bp.relative_address)?
            }

            data.enabled = true;

            // Update the actual session instance
            self.breakpoint_index_tracker[index] = Some(data);
            return Ok(BreakpointMutationResult::Updated);
        }

        Ok(BreakpointMutationResult::NotFound)
    }

    /// Disable breakpoint and returns true if successful
    pub fn disable_breakpoint(
        &mut self,
        index: usize,
    ) -> Result<BreakpointMutationResult, SystemError> {
        // NOTE: Safe index, bounds are checked by the cli
        let target = self.breakpoint_index_tracker[index].clone();

        if let Some(mut data) = target {
            // If already disabled, DO NOTHING
            if !data.enabled {
                return Ok(BreakpointMutationResult::AlreadyInState);
            }

            for bp in data.target.iter() {
                self.clear_specific_breakpoint(bp.relative_address)?;
            }

            data.enabled = false;

            // Update the actual session instance
            self.breakpoint_index_tracker[index] = Some(data);
            return Ok(BreakpointMutationResult::Updated);
        }

        Ok(BreakpointMutationResult::NotFound)
    }

    /// Deletes breakpoint and returns true if successful
    pub fn delete_breakpoint(
        &mut self,
        index: usize,
    ) -> Result<BreakpointMutationResult, SystemError> {
        // NOTE: Safe index, bounds are checked by the cli
        let target = self.breakpoint_index_tracker[index].take();

        if let Some(data) = target {
            for bp in data.target.iter() {
                self.clear_specific_breakpoint(bp.relative_address)?
            }
            return Ok(BreakpointMutationResult::Updated);
        }

        Ok(BreakpointMutationResult::NotFound)
    }

    /// Create breakpoint(s) at a file on a given line number
    /// Returns the number of breakpoint targets that were found on the given line alongside the address/first target if multiple addresses exist
    pub fn create_breakpoint(
        &mut self,
        line_number: u32,
        file: &Path,
    ) -> Result<BreakpointMutationResult, SystemError> {
        let Some(line_index) = self.get_breakpoint_target(line_number) else {
            return Ok(BreakpointMutationResult::NotFound);
        };

        let line_index: Vec<BreakpointTarget> = line_index
            .into_iter()
            .filter(|bp| *bp.file == *file)
            .collect();

        let mut bp_for_line = 0;
        for bp in line_index.iter() {
            self.create_specific_breakpoint(bp.relative_address)?;
            bp_for_line += 1;
        }

        self.breakpoint_index_tracker
            .push(Some(BreakpointData::from_target(
                line_index.clone(),
                line_number,
                file,
            )));

        Ok(BreakpointMutationResult::Created {
            count: bp_for_line as u8,
            target: line_index[0].clone(),
        })
    }

    /// Get the current index of the breakpoint the user is currently on
    pub fn current_index(&self) -> usize {
        // Index is one based for the user
        self.breakpoint_index_tracker.len()
    }

    /// Clear breakpoint at specfic breakpoint address
    pub fn clear_specific_breakpoint(&mut self, relative_address: u64) -> Result<(), SystemError> {
        let absolute_address = self.get_absolute_address(relative_address);
        let mut should_remove = false;

        // If breakpoint doesnt exist, simply ignore it
        if let Some(managed_breakpoint) = self.active_breakpoints.get_mut(&absolute_address) {
            if managed_breakpoint.ref_count > 1 {
                // Other breakpoints exist, dont remove it, simply decrement
                managed_breakpoint.ref_count -= 1;
            } else {
                managed_breakpoint.breakpoint.disable(self.pid)?;
                should_remove = true;
            }
        }

        if should_remove {
            self.active_breakpoints.remove(&absolute_address);
        }

        Ok(())
    }

    /// Get absolute address ( the sum of base address and absolute address )
    pub fn get_absolute_address(&self, relative_address: u64) -> u64 {
        self.base_address + relative_address
    }

    /// Get breakpoint target (file name and relative address ) from line number and file name
    pub fn get_specific_breakpoint_target(
        &self,
        file_name: &str,
        line_number: u32,
    ) -> Vec<BreakpointTarget> {
        let Some(line_index) = self.get_breakpoint_target(line_number) else {
            return vec![];
        };

        line_index
            .into_iter()
            .filter(|x| x.file.ends_with(file_name))
            .collect()
    }

    /// Get breakpoint target (file name and relative_address ) from the just line number
    pub fn get_breakpoint_target(&self, line_number: u32) -> Option<Vec<BreakpointTarget>> {
        let line_index = self.line_index.get(&line_number);
        line_index.cloned()
    }
}
