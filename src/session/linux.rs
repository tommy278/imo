use nix::sys::ptrace::AddressType;
use nix::{libc::user_regs_struct, sys::ptrace};
use std::fs::read_to_string;

// Define the platform aliases exposed to mod.rs
pub type ProcessId = nix::unistd::Pid;
pub type PlatformBreakpoint = crate::interface::linux::BreakPoint;
pub type PlatformRegStruct = user_regs_struct;

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

/// Send a SIGSTOP signal to the main loop
/// Main loop decides how to step ( Only notifies )
pub fn begin_step_process(pid: ProcessId) {
    // Detach and re-attach to the send a SIGSTOP signal
    ptrace::detach(pid, None).unwrap();
    ptrace::attach(pid).unwrap();
}

/// Proceed forward when the process is stopped
pub fn step(pid: ProcessId) {
    ptrace::step(pid, None).unwrap();
}

/// Get all register data
pub fn get_regs(pid: ProcessId) -> PlatformRegStruct {
    ptrace::getregs(pid).unwrap()
}

pub fn peek_data(pid: ProcessId, address: u64) -> i64 {
    ptrace::read(pid, address as AddressType).unwrap()
}

/* pub fn peek_data(pid: ProcessId, address: u64) -> u32 {
    let mut mem_file = File::open(format!("/proc/{}/mem", pid)).unwrap();
    let mut buffer = [0u8; 4];

    mem_file.read_exact_at(&mut buffer, address).unwrap();
    u32::from_le_bytes(buffer)
} */
