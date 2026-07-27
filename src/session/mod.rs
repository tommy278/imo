pub mod interface;
#[cfg(target_os = "linux")]
pub mod linux;

use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::helpers::dwarf::{
    self,
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

/// Cache for entire debug session
#[derive(Debug)]
pub struct DebugSession {
    // Breakpoint data
    pub line_index: FxHashMap<u64, Vec<BreakpointTarget>>,
    pub base_address: u64,
    pub breakpoint_index_tracker: Vec<Option<BreakpointData>>,
    pub address_to_location: FxHashMap<u64, SourceLocation>,

    // Rust does not delcare variables in sequential order
    // Used to find out the actual order in while files were declared
    pub file_declaration_order: FxHashMap<PathBuf, Vec<u64>>,

    // Metdata
    pub metadata: DebuggerMetadataCache,

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

        session
    }

    pub fn get_regs(&self) -> RegisterViewer {
        let regs = os::get_regs(self.pid);
        RegisterViewer::new(regs)
    }

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

    /// Continue session from last interrupt
    pub fn continue_session(&self) {
        os::continue_session(self.pid);
    }

    pub fn begin_step_process(&self) {
        os::begin_step_process(self.pid);
    }

    /// Move forward from the specified stop
    pub fn step(&self) {
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

    pub fn get_location_with_address(&self, relative_address: u64) -> Option<&SourceLocation> {
        self.address_to_location.get(&relative_address)
    }

    pub fn get_file_decl_order(&self, file: PathBuf) -> Option<&Vec<u64>> {
        self.file_declaration_order.get(&file)
    }

    pub fn get_relative_address(&self, absolute_address: u64) -> u64 {
        absolute_address - self.base_address
    }

    // pub fn debug(&self, node: &debug_info::ScopeCacheNode) {
    //     let regs = self.get_regs();

    //     let endian = &self.metadata.endian;
    //     let encoding = self.metadata.encoding.unwrap();
    //     let abi = &self.metadata.abi;

    //     let addresses = node.get_addresses(&regs, encoding, *endian, abi);

    //     addresses.iter().for_each(|add| {
    //         let data = os::peek_data(self.pid, *add);
    //         println!("{}", data);
    //     });
    // }

    pub fn find_current_scope(&self) -> Option<ActiveVariablesContext<'_>> {
        let current_pc = self.get_regs().regs.rip - self.base_address;
        self.metadata.find_scope_by_pc(current_pc)
    }

    /// Get the value of a variable with the given name
    pub fn get_var_value(&self, node: &ActiveVariablesContext, name: &str) -> Option<DebugValue> {
        let regs = self.get_regs();

        let endian = self.metadata.endian;
        let encoding = self.metadata.encoding.unwrap();
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

            let address = variable
                .parse_value(
                    &regs,
                    encoding,
                    endian,
                    abi,
                    node.frame_base?,
                    &self.metadata.type_index,
                    self.pid,
                )
                .unwrap();

            if let Some(ty) = self.metadata.type_index.get(&variable.target_type_offset) {
                return ty
                    .dwarf_type
                    .to_debug_value(&self.metadata.type_index, address, self.pid);
            }
        }
        None
    }

    // pub fn get_scope_info(&self) -> Option<&ScopeCacheNode> {
    //     let regs = self.get_regs().regs;

    //     let current_pc = regs.rip - self.base_address;

    //     if let Some(scope_idx) = self.metadata.find_scope_by_pc(current_pc) {
    //         let active_node = &self.metadata.execution_scopes[scope_idx];
    //         return Some(active_node);
    //     }
    //     None
    // }

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

    pub fn current_index(&self) -> usize {
        self.breakpoint_index_tracker.len()
    }

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
