use nix::sys::ptrace;
use nix::sys::ptrace::AddressType;
use nix::sys::signal;
use nix::sys::uio::{RemoteIoVec, process_vm_readv};
use std::fs::read_to_string;
use std::io::IoSliceMut;

use crate::dwarf::error::CacheSetupError;
use crate::sys::linux::error::LinuxError;
use crate::sys::linux::{PlatformRegStruct, ProcessId};
use crate::sys::ProcessAddressRange;

/// Get process base address
pub fn get_process_address_range(pid: ProcessId) -> Result<ProcessAddressRange, CacheSetupError> {
    let maps_path = format!("/proc/{}/maps", pid);
    if let Ok(content) = read_to_string(maps_path) {
        if let Some(first_line) = content.lines().next() {
            let mut target_line = first_line.split('-');
            if let Some(base_str) = target_line.next() {
                if let Some(max_str) = target_line.next() {
                    let Ok(base_address) = u64::from_str_radix(base_str, 16) else {
                        return Err(CacheSetupError::AddressRange);
                    };

                    let Ok(max_address) = u64::from_str_radix(&max_str[..1], 16) else {
                        return Err(CacheSetupError::AddressRange);
                    };

                    return Ok(ProcessAddressRange { base_address, max_address });
                }
            }
        }
    }
    Err(CacheSetupError::AddressRange)
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

    let bytes_read = process_vm_readv(pid, &mut [local_iov], &[remote_iov])?;

    if bytes_read == len {
        return Ok(buffer);
    }

    Err(LinuxError::ByteRead)
}
