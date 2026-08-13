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
pub fn get_process_base_address(pid: ProcessId) -> Result<u64, CacheSetupError> {
    let maps_path = format!("/proc/{}/maps", pid);
    if let Ok(content) = read_to_string(maps_path) {
        if let Some(first_line) = content.lines().next() {
            if let Some(base_str) = first_line.split('-').next() {
                return u64::from_str_radix(base_str, 16).map_err(|_| CacheSetupError::BaseAddress);
            }
        }
    }
    Err(CacheSetupError::BaseAddress)
}

/// Continue debug session
pub fn continue_session(pid: ProcessId) -> Result<(), LinuxError> {
    ptrace::cont(pid, None)?;
    Ok(())
}

/// Kill debug session
pub fn kill_session(pid: ProcessId) -> Result<(), LinuxError> {
    ptrace::kill(pid)?;
    Ok(())
}

pub fn send_trap_signal(pid: ProcessId) -> Result<(), LinuxError> {
    signal::kill(pid, signal::Signal::SIGTRAP)?;
    Ok(())
}

/// Proceed forward when the process is stopped
pub fn step(pid: ProcessId) -> Result<(), LinuxError> {
    ptrace::step(pid, None)?;
    Ok(())
}

/// Get all register data
pub fn get_regs(pid: ProcessId) -> Result<PlatformRegStruct, LinuxError> {
    let regs = ptrace::getregs(pid)?;
    Ok(regs)
}

pub fn peek_data(pid: ProcessId, address: u64) -> Result<i64, LinuxError> {
    let data = ptrace::read(pid, address as AddressType)?;
    Ok(data)
}

pub fn read_bytes(
    pid: ProcessId,
    remote_address: usize,
    len: usize,
) -> Result<Vec<u8>, LinuxError> {
    let mut buffer = vec![0u8; len];
    let local_iov = IoSliceMut::new(&mut buffer);

    let remote_iov = RemoteIoVec {
        base: remote_address,
        len,
    };

    let bytes_read = process_vm_readv(pid, &mut [local_iov], &[remote_iov]).unwrap();

    if bytes_read == len {
        return Ok(buffer);
    }

    Err(LinuxError::ByteRead)
}

// Linux specific errors

use thiserror::Error;

use crate::helpers::dwarf::error::CacheSetupError;

#[derive(Debug, Error)]
pub enum LinuxError {
    #[error("Failed to execute ptrace command. Errno: {0}")]
    Ptrace(#[from] nix::errno::Errno),
    #[error("Failed to read bytes at given address")]
    ByteRead,
}
