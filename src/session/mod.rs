#[cfg(target_os = "linux")]
pub mod linux;

use rustc_hash::FxHashMap;
use std::{path::Path, rc::Rc};

use crate::helpers::dwarf;
use crate::helpers::trim_file_path;

use crate::interface::RegisterViewer;
#[cfg(target_os = "linux")]
use crate::session::linux as os;

// If not supported yet, add dummy values for compilation
#[cfg(not(target_os = "linux"))]
mod os {
    use std::path::Path;
    use std::rc::Rc;

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

#[derive(Debug, Clone)]
pub struct BreakpointTarget {
    pub file: Rc<Path>,
    pub relative_address: u64,
}

#[derive(Debug, Clone)]
pub struct BreakpointData {
    pub target: Vec<BreakpointTarget>,
    pub line: u64,
    pub file: Box<Path>,
    pub enabled: bool,
}

impl BreakpointData {
    pub fn from_target(target: Vec<BreakpointTarget>, line: u64, file: &Path) -> Self {
        Self {
            target,
            line,
            file: Box::from(file),
            enabled: true,
        }
    }
}

impl std::fmt::Display for BreakpointData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let enabled = if self.enabled { "y" } else { "n" };

        if self.target.len() == 1 {
            let target = &self.target[0];
            let file_path = trim_file_path(&target.file);

            write!(
                f,
                "breakpoint\tkeep {}\t0x{:016x} at {}:{}",
                enabled, target.relative_address, file_path, self.line
            )?;
        } else {
            writeln!(f, "breakpoint\tkeep {}\t<MULTIPLE>", enabled)?;

            for (idx, target) in self.target.iter().enumerate() {
                let user_idx = idx + 1;
                write!(
                    f,
                    "  .{}\t\t     {}\t0x{:016x} at {}:{}",
                    user_idx,
                    enabled,
                    target.relative_address,
                    target.file.display(),
                    self.line
                )?;

                // Avoid an extra trailing newline at the very last location
                if idx < self.target.len() - 1 {
                    writeln!(f)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ManagedBreakpoint {
    pub breakpoint: os::PlatformBreakpoint,
    pub ref_count: usize,
}

#[derive(Debug)]
pub enum BreakpointMutationResult {
    Updated,
    AlreadyInState,
    NotFound,
}

impl ManagedBreakpoint {
    pub fn new(breakpoint: os::PlatformBreakpoint) -> Self {
        Self {
            breakpoint,
            ref_count: 1,
        }
    }
}

/// Cache for entire debug session
#[derive(Debug)]
pub struct DebugSession {
    pub line_index: FxHashMap<u64, Vec<BreakpointTarget>>,
    pub base_address: u64,
    pub breakpoint_index_tracker: Vec<Option<BreakpointData>>,

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

        // Update line index and address to location
        dwarf::init::setup_session_cache(binary_path, &mut session);

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
