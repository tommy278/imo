pub mod interface;
#[cfg(target_os = "linux")]
pub mod linux;

use gimli::UnwindSection;
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::helpers::dwarf::{
    self,
    debug_frame::{RawDebugFrame, setup_session_debug_frame},
    debug_info::{ActiveVariablesContext, DebuggerMetadataCache},
};
use crate::interface::{DebugValue, RegisterViewer};
use crate::session::interface::{BreakpointData, BreakpointMutationResult, BreakpointTarget};

#[cfg(target_os = "linux")]
use crate::session::linux as os;

// If not supported yet, add dummy values for compilation
#[cfg(not(target_os = "linux"))]
mod os {
    // Dummy types to satisfy the type aliases
    pub type ProcessId = i32;
    pub type PlatformRegStruct = ();

    #[derive(Debug, Clone)]
    pub struct PlatformBreakpoint;

    impl PlatformBreakpoint {
        pub fn new(_absolute_address: u64) -> Self {
            unimplemented!("imo debugger only runs on linux")
        }
        pub fn enable(&self, _pid: ProcessId) {
            unimplemented!("imo debugger only runs on linux")
        }
        pub fn disable(&self, _pid: ProcessId) {
            unimplemented!("imo debugger only runs on linux")
        }
    }

    pub fn get_process_base_address(_pid: ProcessId) -> u64 {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn begin_step_process(_pid: ProcessId) {
        unimplemented!("imo debugger only runs on linux")
    }

    pub fn step(_pid: ProcessId) {
        unimplemented!("imo deb")
    }

    pub fn continue_session(_pid: ProcessId) {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn kill_session(_pid: ProcessId) {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn get_regs(_pid: ProcessId) -> PlatformRegStruct {
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

#[derive(Debug)]
pub struct SourceLocation {
    pub file: Rc<Path>,
    pub line: u64,
}

// The command to be ran when the debugger hits a sigtrap
#[derive(Default, Debug)]
pub enum CurrentStopCmd {
    SingleStep,
    StepOver {
        start_cfa: u64,
        start_line: u64,
        start_file: PathBuf,
    },
    StepInto {
        start_file: Rc<Path>,
        start_line: u64,
    },
    StepOut {
        resume_cfa: u64,
        start_line: u64,
        start_file: PathBuf,
    },
    #[default]
    Idle,
    Running,
    Continuing,
    SearchingForValidLocation,
    SearchingForValidStartLocation,
    SearchingForNextValidLocation {
        start_line: u64,
        start_file: PathBuf,
    },
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

/// Cache for entire debug session
#[derive(Debug)]
pub struct DebugSession {
    // Breakpoint data
    pub line_index: FxHashMap<u64, Vec<BreakpointTarget>>,
    pub base_address: u64,
    pub breakpoint_index_tracker: Vec<Option<BreakpointData>>,
    pub address_to_location: FxHashMap<u64, SourceLocation>,

    // Used to find out the actual order in while files were declared
    pub file_declaration_order: FxHashMap<PathBuf, Vec<u64>>,

    // Metdata
    pub metadata: DebuggerMetadataCache,

    // Tracking the current state the debugger is in
    pub current_cmd: CurrentStopCmd,

    // Debug frame for finding CFA
    pub raw_debug_frame: RawDebugFrame,

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
            address_to_location: FxHashMap::default(),
            file_declaration_order: FxHashMap::default(),
            metadata: DebuggerMetadataCache::default(),
            raw_debug_frame: RawDebugFrame::default(),
            current_cmd: CurrentStopCmd::default(),
            active_breakpoints: FxHashMap::default(),
            pid,
        }
    }

    // =================================================================
    // OS Specific functions
    // =================================================================

    /// Create a complete instance of the session cache
    pub fn new(pid: os::ProcessId, binary_path: &str) -> Self {
        let mut session = Self::from_pid(pid);

        session.update_process_base_address();

        session.metadata = DebuggerMetadataCache::new(binary_path);

        // Update line index and address to location
        dwarf::debug_line::setup_session_cache(binary_path, &mut session);

        session.raw_debug_frame = setup_session_debug_frame(binary_path);

        session
    }

    /// Get the live register of the current process
    pub fn get_regs(&self) -> RegisterViewer {
        let regs = os::get_regs(self.pid);
        RegisterViewer::new(regs)
    }

    /// Create a specific breakpoint at a given address
    pub fn create_specific_breakpoint(&mut self, relative_address: u64) {
        let absolute_address = self.get_absolute_address(relative_address);

        // If breakpoint already exists dont write simply increment the reference counter
        if let Some(managed_breakpoint) = self.active_breakpoints.get_mut(&absolute_address) {
            managed_breakpoint.ref_count += 1;
            return;
        }

        // First time seeing the address
        // Create the breakpoint
        let mut breakpoint = os::PlatformBreakpoint::new(absolute_address);
        breakpoint.enable(self.pid);

        self.active_breakpoints
            .insert(absolute_address, ManagedBreakpoint::new(breakpoint));
    }

    pub fn get_unwind_table(&self) -> gimli::EhFrame<gimli::EndianSlice<'_, gimli::RunTimeEndian>> {
        self.raw_debug_frame
            .get_unwind_table_with_endian(self.metadata.endian)
    }

    pub fn get_register_value(&self, register: gimli::Register) -> u64 {
        let regs = self.get_regs().regs;

        match register.0 {
            6 => regs.rbp,
            7 => regs.rsp,
            16 => regs.rip,
            _ => todo!("Not implemented yet {}", register.0),
        }
    }

    pub fn get_cfa_and_ret_addr(&self) -> Option<(u64, u64)> {
        let eh_frame = self.get_unwind_table();
        let base_addresses = self.metadata.base_addresses.clone();

        let current_pc = self.current_rip() - self.base_address;

        if let Ok(fde) =
            eh_frame.fde_for_address(&base_addresses, current_pc, |sections, bases, offset| {
                sections.cie_from_offset(bases, offset)
            })
        {
            let mut ctx = gimli::UnwindContext::new();
            let mut table = fde.rows(&eh_frame, &base_addresses, &mut ctx).unwrap();

            let ra_register = fde.cie().return_address_register();
            while let Some(row) = table.next_row().unwrap() {
                let cfa_address = match row.cfa() {
                    gimli::CfaRule::RegisterAndOffset { register, offset } => {
                        let reg_value = self.get_register_value(*register);
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
                                    crate::session::linux::peek_data(self.pid, ra_storage_address)
                                        as u64;
                                return Some((cfa_address, return_address));
                            }
                            gimli::RegisterRule::Register(saved_reg) => {
                                let return_address = self.get_register_value(saved_reg);
                                return Some((cfa_address, return_address));
                            }
                            _ => todo!(),
                        }
                    }
                }
            }
        }
        None
    }

    pub fn current_rip(&self) -> u64 {
        self.get_regs().regs.rip
    }

    pub fn is_idle(&self) -> bool {
        self.current_cmd.is_idle()
    }

    pub fn toggle_running(&mut self) {
        self.current_cmd = CurrentStopCmd::Running;
    }

    /// Continue session from last interrupt
    pub fn continue_session(&self) {
        os::continue_session(self.pid);
    }

    pub fn send_trap_signal(&self) {
        os::send_trap_signal(self.pid);
    }

    pub fn toggle_continue(&mut self) {
        self.current_cmd = CurrentStopCmd::Continuing;
    }

    pub fn complete_single_step(&mut self) {
        self.current_cmd = CurrentStopCmd::SingleStep;
        self.single_step();
    }

    pub fn begin_step_into(&mut self) {
        let Some(current_location) = self.current_location() else {
            self.current_cmd = CurrentStopCmd::SearchingForValidLocation;
            self.single_step();
            return;
        };

        self.current_cmd = CurrentStopCmd::StepInto {
            start_file: current_location.file.clone(),
            start_line: current_location.line,
        };
        self.single_step();
    }

    pub fn begin_step_over(&mut self) {
        let Some(current_location) = self.current_location() else {
            self.current_cmd = CurrentStopCmd::SearchingForValidStartLocation;
            self.single_step();
            return;
        };
        let (current_cfa, _) = self.get_cfa_and_ret_addr().unwrap();

        self.current_cmd = CurrentStopCmd::StepOver {
            start_cfa: current_cfa,
            start_line: current_location.line,
            start_file: current_location.file.to_path_buf(),
        };
        self.single_step();
    }

    pub fn current_location(&self) -> Option<&SourceLocation> {
        let abs = self.get_regs().regs.rip;
        let rel_addr = self.get_relative_address(abs);
        self.get_location_with_address(rel_addr)
    }

    /// Move forward from the specified stop
    pub fn single_step(&self) {
        os::step(self.pid);
    }

    /// Kill the current session
    pub fn kill_session(&self) {
        os::kill_session(self.pid);
    }

    /// Get and update the process base address
    pub fn update_process_base_address(&mut self) {
        self.base_address = os::get_process_base_address(self.pid);
    }

    // =================================================================
    // Other Methods
    // =================================================================

    /// Obtain the location that an address belongs to within the program
    pub fn get_location_with_address(&self, relative_address: u64) -> Option<&SourceLocation> {
        self.address_to_location.get(&relative_address)
    }

    /// Get the exact order in which the compiler actually initialized the variables
    /// Rust does not always initialize variables sequentially
    pub fn get_file_decl_order(&self, file: PathBuf) -> Option<&Vec<u64>> {
        self.file_declaration_order.get(&file)
    }

    /// Get the relative address from absolute address
    pub fn get_relative_address(&self, absolute_address: u64) -> u64 {
        absolute_address - self.base_address
    }

    /// Find current scope with internal pc
    pub fn find_current_scope(&self) -> ActiveVariablesContext<'_> {
        let current_pc = self.get_regs().regs.rip - self.base_address;
        self.metadata.find_scope_by_pc(current_pc)
    }

    /// Get the value of a variable with the given name
    /// Requires current scope to evaluate the value
    pub fn get_var_value(&self, node: &ActiveVariablesContext, name: &str) -> Option<DebugValue> {
        let regs = self.get_regs();

        let endian = self.metadata.endian;
        let encoding = self.metadata.encoding?;
        let abi = &self.metadata.abi;

        // Since rust does not declare variables in sequential order
        // We store the actual order the variables were actually declared
        // We use this to check if the decl_line is before our current index
        // If it is that means it has been declared and if not then it hasnt

        if let Some(variable) = node.get_variable_with_name(name) {
            let current_pc = regs.regs.rip - self.base_address;

            if let Some(info) = self.address_to_location.get(&current_pc) {
                let SourceLocation { file, line } = info;
                let file = file.to_path_buf();

                if let Some(line_order) = self.get_file_decl_order(file) {
                    if let Some(current_idx) = line_order.iter().position(|&l| l == *line) {
                        if let Some(var_decl_idx) =
                            line_order.iter().position(|&l| l == variable.decl_line)
                        {
                            if var_decl_idx >= current_idx {
                                return Some(DebugValue::Err(
                                    "Variable not initialized yet".to_string(),
                                ));
                            }
                        }
                    }
                }
            } else {
                return None;
            }

            // Get the variable's address
            let address = variable.parse_value(
                &regs,
                encoding,
                endian,
                abi,
                node.frame_base?,
                &self.metadata.type_index,
                self.pid,
            )?;

            // Resolve the variable's live value with address and current pid
            if let Some(ty) = self.metadata.type_index.get(&variable.target_type_offset) {
                return ty
                    .dwarf_type
                    .to_debug_value(&self.metadata.type_index, address, self.pid);
            }
        }
        None
    }

    /// Clear all breakpoint for line_number by default
    /// Only clear specified breakpoints if file name is provided
    pub fn clear_breakpoint(&mut self, line_number: u64, file: Option<&str>) -> Vec<usize> {
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
                    let bp_file = bp.file.to_str().expect("Could not convert path");

                    // Safe unwrap since this is the path where the file is Some
                    if bp.line == line_number && bp_file.ends_with(file.unwrap()) {
                        if let Some(removed_bp) = opt_bp.take() {
                            cleared_breakpoints.push(removed_bp);
                            bp_idx.push(idx + 1);
                        }
                    }
                }
            }
        }

        // Clear every breakpoint
        cleared_breakpoints.iter().for_each(|data| {
            data.target.iter().for_each(|bp| {
                self.clear_specific_breakpoint(bp.relative_address);
            });
        });

        bp_idx
    }

    /// Enable breakpoint at a specific index in the tracker
    pub fn enable_breakpoint(&mut self, index: usize) -> BreakpointMutationResult {
        // NOTE: Safe index, bounds are checked by the cli
        let target = self.breakpoint_index_tracker[index].clone();

        if let Some(mut data) = target {
            // If already enabled, DO NOTHING
            if data.enabled {
                return BreakpointMutationResult::AlreadyInState;
            }

            data.target.iter().for_each(|bp| {
                self.create_specific_breakpoint(bp.relative_address);
            });

            data.enabled = true;

            // Update the actual session instance
            self.breakpoint_index_tracker[index] = Some(data);
            return BreakpointMutationResult::Updated;
        }

        BreakpointMutationResult::NotFound
    }

    /// Disable breakpoint and returns true if successful
    pub fn disable_breakpoint(&mut self, index: usize) -> BreakpointMutationResult {
        // NOTE: Safe index, bounds are checked by the cli
        let target = self.breakpoint_index_tracker[index].clone();

        if let Some(mut data) = target {
            // If already disabled, DO NOTHING
            if !data.enabled {
                return BreakpointMutationResult::AlreadyInState;
            }

            data.target.iter().for_each(|bp| {
                self.clear_specific_breakpoint(bp.relative_address);
            });

            data.enabled = false;

            // Update the actual session instance
            self.breakpoint_index_tracker[index] = Some(data);
            return BreakpointMutationResult::Updated;
        }

        BreakpointMutationResult::NotFound
    }

    /// Deletes breakpoint and returns true if successful
    pub fn delete_breakpoint(&mut self, index: usize) -> BreakpointMutationResult {
        // NOTE: Safe index, bounds are checked by the cli
        let target = self.breakpoint_index_tracker[index].take();

        if let Some(data) = target {
            data.target.iter().for_each(|bp| {
                self.clear_specific_breakpoint(bp.relative_address);
            });
            return BreakpointMutationResult::Updated;
        }

        BreakpointMutationResult::NotFound
    }

    /// Create breakpoint(s) at a file on a given line number
    /// Returns the number of breakpoint targets that were found on the given line alongside the address/first target if multiple addresses exist
    pub fn create_breakpoint(&mut self, line_number: u64, file: &Path) -> (u64, BreakpointTarget) {
        let line_index = self.get_breakpoint_target(line_number).unwrap();

        let line_index: Vec<BreakpointTarget> = line_index
            .into_iter()
            .filter(|bp| *bp.file == *file)
            .collect();

        let mut bp_for_line = 0;
        line_index.iter().for_each(|bp| {
            self.create_specific_breakpoint(bp.relative_address);
            bp_for_line += 1;
        });

        self.breakpoint_index_tracker
            .push(Some(BreakpointData::from_target(
                line_index.clone(),
                line_number,
                file,
            )));

        (bp_for_line, line_index[0].clone())
    }

    /// Get the current index of the breakpoint the user is currently on
    pub fn current_index(&self) -> usize {
        // Index is one based for the user
        self.breakpoint_index_tracker.len()
    }

    /// Clear breakpoint at specfic breakpoint address
    pub fn clear_specific_breakpoint(&mut self, relative_address: u64) {
        let absolute_address = self.get_absolute_address(relative_address);
        let mut should_remove = false;

        // If breakpoint doesnt exist, simply ignore it
        if let Some(managed_breakpoint) = self.active_breakpoints.get_mut(&absolute_address) {
            if managed_breakpoint.ref_count > 1 {
                // Other breakpoints exist, dont remove it, simply decrement
                managed_breakpoint.ref_count -= 1;
            } else {
                managed_breakpoint.breakpoint.disable(self.pid);
                should_remove = true;
            }
        }

        if should_remove {
            self.active_breakpoints.remove(&absolute_address);
        }
    }

    /// Get absolute address ( the sum of base address and absolute address )
    pub fn get_absolute_address(&self, relative_address: u64) -> u64 {
        self.base_address + relative_address
    }

    /// Get breakpoint target (file name and relative address ) from line number and file name
    pub fn get_specific_breakpoint_target(
        &self,
        file_name: &str,
        line_number: u64,
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
    pub fn get_breakpoint_target(&self, line_number: u64) -> Option<Vec<BreakpointTarget>> {
        let line_index = self.line_index.get(&line_number);
        line_index.cloned()
    }
}
