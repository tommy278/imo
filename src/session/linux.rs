use crate::session::DebugSession;
use nix::sys::ptrace;
use std::fs::read_to_string;

// Define the platform aliases exposed to mod.rs
pub type ProcessId = nix::unistd::Pid;
pub type PlatformBreakpoint = crate::interface::linux::BreakPoint;

/// Get process base address
pub fn get_process_base_address(pid: ProcessId) -> u64 {
    let maps_path = format!("/proc/{}/maps", pid);
    if let Ok(content) = read_to_string(maps_path) {
        if let Some(first_line) = content.lines().next() {
            if let Some(base_str) = first_line.split('-').next() {
                return u64::from_str_radix(base_str, 16).unwrap_or_default();
            }
        }
    }
    0
}

/// Continue debug session
pub fn continue_session(pid: ProcessId) {
    ptrace::cont(pid, None).unwrap();
}

/// Kill debug session
pub fn kill_session(pid: ProcessId) {
    ptrace::kill(pid).unwrap()
}
