use nix::sys::ptrace::AddressType;
use nix::sys::uio::{RemoteIoVec, process_vm_readv};
use nix::{libc::user_regs_struct, sys::ptrace};
use std::fs::File;
use std::fs::read_to_string;
use std::io::IoSliceMut;
use std::io::{BufRead, BufReader};

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
    ptrace::read(pid, address as AddressType).expect("Could not read address")
}

pub fn read_bytes(pid: ProcessId, remote_address: usize, len: usize) -> Option<Vec<u8>> {
    // TODO: Add an actual robust way to check if the address is valid
    if len > 4096 * 4096 {
        return None;
    }
    let mut buffer = vec![0u8; len];
    let local_iov = IoSliceMut::new(&mut buffer);

    let remote_iov = RemoteIoVec {
        base: remote_address,
        len,
    };

    let bytes_read = process_vm_readv(pid, &mut [local_iov], &[remote_iov]).unwrap();

    if bytes_read == len {
        return Some(buffer);
    }

    unimplemented!("Error reading bytes");
}
