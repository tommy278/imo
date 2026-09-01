use nix::sys::ptrace;
use nix::sys::ptrace::AddressType;
use nix::sys::signal;
use nix::sys::uio::{process_vm_readv, RemoteIoVec};
use std::fs::read_to_string;
use std::io::IoSliceMut;

use crate::dwarf::error::CacheSetupError;
use crate::sys::linux::error::LinuxError;
use crate::sys::linux::{PlatformRegStruct, ProcessId};
use crate::sys::{MemoryRegion, ProcessMemoryMap};

/// Get process base address
pub fn update_process_addresses(
    session: &mut crate::session::DebugSession,
    pid: ProcessId,
) -> Result<(), CacheSetupError> {
    let maps_path = format!("/proc/{}/maps", pid);
    if let Ok(content) = read_to_string(maps_path) {
        let mut content_iter = content.lines();

        let mut regions = Vec::new();

        while let Some(line) = content_iter.next() {
            let mut is_executable = false;
            let mut is_writable = false;
            let mut is_readable = false;

            if let Some((base_str, end_str)) = line.split_once('-') {
                let Ok(base_address) = u64::from_str_radix(base_str, 16) else {
                    return Err(CacheSetupError::AddressRange);
                };

                if session.base_address == 0 {
                    session.base_address = base_address;
                }

                let mut other_end = end_str.split_whitespace();

                if let Some(stripped_end_str) = other_end.next() {
                    if let Some(perm) = other_end.next() {
                        if perm.contains("r") {
                            is_readable = true;
                        }

                        if perm.contains("x") {
                            is_executable = true;
                        }

                        if perm.contains("w") {
                            is_writable = true;
                        }
                    }

                    let Ok(end_address) = u64::from_str_radix(stripped_end_str, 16) else {
                        return Err(CacheSetupError::AddressRange);
                    };

                    regions.push(MemoryRegion {
                        start_address: base_address,
                        end_address,
                        is_writable,
                        is_readable,
                        is_executable,
                    });
                }
            }
        }

        session.process_map = ProcessMemoryMap::from(regions);
    }

    Ok(())
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

    if len == 0 {
        return Err(LinuxError::ByteRead);
    }

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
