use crate::dwarf::error::CacheSetupError;
use crate::sys::macos::error::MacOSError;
use crate::sys::macos::{PlatformRegStruct, ProcessHandle};

use std::mem;

use mach2::{thread_act, vm};

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use mach2::structs::x86_thread_state64_t;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use mach2::thread_status::x86_THREAD_STATE64 as THREAD_STATE_64;

pub fn send_trap_signal(_port: ProcessHandle) -> Result<(), MacOSError> {
    unimplemented!("imo debugger only runs on Linux")
}

pub fn update_process_addresses(
    _: &mut crate::session::DebugSession,
) -> Result<(), CacheSetupError> {
    Ok(())
}

pub fn read_bytes(port: ProcessHandle, ptr: usize, len: usize) -> Result<Vec<u8>, MacOSError> {
    let mut buffer = vec![0u8; len];
    let mut out_size = 0;

    unsafe {
        vm::mach_vm_read_overwrite(
            port,
            ptr as u64,
            len as u64,
            mem::transmute(&buffer),
            mem::transmute(&out_size),
        );
    }

    Ok(buffer)
}

pub fn step(_port: ProcessHandle) -> Result<(), MacOSError> {
    unimplemented!("imo debugger only runs on Linux")
}

pub fn continue_session(_port: ProcessHandle) -> Result<(), MacOSError> {
    unimplemented!("imo debugger only runs on Linux")
}

pub fn kill_session(_port: ProcessHandle) -> Result<(), MacOSError> {
    unimplemented!("imo debugger only runs on Linux")
}

pub fn peek_data(port: ProcessHandle, address: u64) -> Result<i64, MacOSError> {
    let raw_bytes = read_bytes(port, address as usize, 8)?;

    if raw_bytes.len() != 8 {
        todo!()
    }

    let bytes: [u8; 8] = raw_bytes.try_into().unwrap();
    let data = i64::from_le_bytes(bytes);

    Ok(data)
}

pub fn get_regs(port: ProcessHandle) -> Result<PlatformRegStruct, MacOSError> {
    let mut regs_state = x86_thread_state64_t::new();
    let mut state_count = x86_thread_state64_t::count();

    unsafe {
        thread_act::thread_get_state(
            port,
            THREAD_STATE_64,
            mem::transmute(&regs_state),
            mem::transmute(&state_count),
        );
    }

    Ok(regs_state)
}
