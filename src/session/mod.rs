pub mod linux;

use rustc_hash::FxHashMap;
use std::{path::Path, rc::Rc};

use crate::helpers::dwarf;

#[cfg(target_os = "linux")]
use crate::session::linux as os;

#[derive(Debug)]
pub struct SourceLocation {
    pub file: Rc<Path>,
    pub line: u64,
}

#[derive(Debug, Clone)]
pub struct BreakpointTarget {
    pub file: Rc<Path>,
    pub relative_address: u64,
}

#[derive(Debug)]
pub struct DebugSession {
    pub line_index: FxHashMap<u64, Vec<BreakpointTarget>>,
    pub address_to_location: FxHashMap<u64, SourceLocation>,
    pub base_address: u64,

    // Different for each os
    pub active_breakpoints: FxHashMap<u64, os::PlatformBreakpoint>,
    pub pid: os::ProcessId,
}

impl DebugSession {
    /// Instantiate the struct with default values
    fn from_pid(pid: nix::unistd::Pid) -> Self {
        Self {
            base_address: 0,
            line_index: FxHashMap::default(),
            address_to_location: FxHashMap::default(),
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

    pub fn create_breakpoint(&mut self, relative_address: u64) {
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
