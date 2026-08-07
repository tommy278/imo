use iced_x86::Decoder;
use nix::sys::ptrace::AddressType;
use nix::sys::signal;
use nix::sys::uio::{RemoteIoVec, process_vm_readv};
use nix::{libc::user_regs_struct, sys::ptrace};
use std::fs::read_to_string;
use std::io::IoSliceMut;

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

pub fn send_trap_signal(pid: ProcessId) {
    signal::kill(pid, signal::Signal::SIGTRAP).unwrap();
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

pub fn opt_peek_data(pid: ProcessId, address: u64) -> Option<u64> {
    ptrace::read(pid, address as AddressType)
        .ok()
        .map(|v| v as u64)
}

pub fn read_bytes(pid: ProcessId, remote_address: usize, len: usize) -> Option<Vec<u8>> {
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

pub fn get_instruction_info(pid: ProcessId, rip: usize) -> Option<(usize, iced_x86::Code)> {
    let bytes = read_bytes(pid, rip, 15).unwrap();

    if bytes.iter().all(|&b| b == 0) {
        return None;
    }

    let mut decoder = iced_x86::Decoder::with_ip(64, &bytes, 0, iced_x86::DecoderOptions::NONE);
    let instruction = decoder.decode();

    if instruction.is_invalid() {
        return None;
    }

    Some((instruction.len(), instruction.code()))
}
