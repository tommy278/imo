pub mod linux;

use rustc_hash::FxHashMap;
use std::{path::Path, rc::Rc};

use crate::helpers::dwarf;

#[cfg(target_os = "linux")]
use crate::session::linux as os;

#[derive(Debug, Clone)]
pub struct BreakpointTarget {
    pub file: Rc<Path>,
    pub relative_address: u64,
}

#[derive(Debug, Clone)]
pub struct BreakpointData {
    pub target: Vec<BreakpointTarget>,
    pub line: u64,
    pub file: Option<Box<Path>>,
}

impl BreakpointData {
    pub fn from_target(target: Vec<BreakpointTarget>, line: u64, file: Option<&Path>) -> Self {
        let file = file.map(|p| Box::from(p));
        Self { target, line, file }
    }
}

/// Cache for entire debug session
#[derive(Debug)]
pub struct DebugSession {
    pub line_index: FxHashMap<u64, Vec<BreakpointTarget>>,
    pub base_address: u64,
    pub breakpoint_index_tracker: Vec<Option<BreakpointData>>,

    // Different for each os
    pub active_breakpoints: FxHashMap<u64, os::PlatformBreakpoint>,
    pub pid: os::ProcessId,
}

impl DebugSession {
    /// Instantiate the struct with default values
    fn from_pid(pid: nix::unistd::Pid) -> Self {
        Self {
            base_address: 0,
            breakpoint_index_tracker: Vec::new(),
            line_index: FxHashMap::default(),
            active_breakpoints: FxHashMap::default(),
            pid,
        }
    }

    /// Create a complete instance of the session cache
    pub fn new(pid: nix::unistd::Pid, binary_path: &str) -> Self {
        let mut session = Self::from_pid(pid);

        session.update_process_base_address();

        // Update line index and address to location
        dwarf::setup_session_cache(binary_path, &mut session);

        session
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
                    if let Some(bp_file) = &bp.file {
                        let bp_file = bp_file.to_str().expect("Could not convert path");

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
        }

        // Clear every breakpoint
        cleared_breakpoints.iter().for_each(|data| {
            data.target.iter().for_each(|bp| {
                self.clear_specific_breakpoint(bp.relative_address);
            });
        });

        bp_idx
    }

    pub fn create_breakpoint(
        &mut self,
        line_number: u64,
        file: Option<&Path>,
    ) -> (u64, BreakpointTarget) {
        let line_index = self.get_breakpoint_target(line_number).unwrap();

        let line_index = if let Some(file) = file {
            line_index
                .into_iter()
                .filter(|bp| *bp.file == *file)
                .collect()
        } else {
            line_index
        };

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

        // If breakpoint doesnt exist, simply ignore it
        if !self.active_breakpoints.contains_key(&absolute_address) {
            return;
        }

        let mut breakpoint = os::PlatformBreakpoint::new(absolute_address);
        breakpoint.disable(self.pid);
        self.active_breakpoints.remove(&absolute_address);
    }

    pub fn create_specific_breakpoint(&mut self, relative_address: u64) {
        let absolute_address = self.get_absolute_address(relative_address);

        // If breakpoint already exists dont write to it simply exit
        if self.active_breakpoints.contains_key(&absolute_address) {
            return;
        }

        let mut breakpoint = os::PlatformBreakpoint::new(absolute_address);
        breakpoint.enable(self.pid);
        self.active_breakpoints.insert(absolute_address, breakpoint);
    }

    /// Continue session from last interrupt
    pub fn continue_session(&self) {
        os::continue_session(self.pid);
    }

    /// Kill the current session
    pub fn kill_session(&self) {
        os::kill_session(self.pid);
    }

    /// Get and update the process base address
    pub fn update_process_base_address(&mut self) {
        self.base_address = os::get_process_base_address(self.pid);
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
