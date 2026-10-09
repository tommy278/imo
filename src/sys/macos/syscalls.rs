use crate::dwarf::error::CacheSetupError;
use crate::sys::macos::error::MacOSError;
use crate::sys::macos::{PlatformRegStruct, ProcessHandle};

use std::mem;

use mach2::{thread_act, vm};

#[cfg(target_arch = "x86_64")]
use mach2::thread_status::x86_THREAD_STATE64 as THREAD_STATE64;
#[cfg(target_arch = "aarch64")]
use mach2::thread_status::ARM_THREAD_STATE64 as THREAD_STATE64;

#[cfg(target_arch = "aarch64")]
use mach2::structs::arm_thread_state64_t as thread_state64_t;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use mach2::structs::x86_thread_state64_t as thread_state64_t;

pub fn send_trap_signal(_port: ProcessHandle) -> Result<(), MacOSError> {
    unimplemented!("imo debugger only runs on Linux")
}

pub fn update_process_addresses(
    session: &mut crate::session::DebugSession,
) -> Result<(), CacheSetupError> {
    let mut kern_return = mach2::kern_return::KERN_SUCCESS;
    let mut address = 0;
    let mut size = 0;
    let mut depth = 1;

    loop {
        let mut info = mach2::vm_region::vm_region_submap_info_64::default();
        let mut count = mach2::vm_region::VM_REGION_SUBMAP_INFO_COUNT;

        unsafe {
            kern_return = mach2::vm::mach_vm_region_recurse(
                session.process_handle,
                &mut address,
                &mut size,
                &mut depth,
                std::mem::transmute(&info),
                &mut count,
            );
        }

        if kern_return == mach2::kern_return::KERN_INVALID_ADDRESS {
            break;
        }

        if info.is_submap == 1 {
            depth += 1;
        } else {
            if session.base_address == 0 {
                session.base_address = address;
            }
            println!("Address: {} to {}", address, address + size);
            address += size;
        }
    }

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

pub fn continue_session(port: ProcessHandle) -> Result<(), MacOSError> {
    unsafe {
        let kern_return = mach2::task::task_resume(port);
        println!("Task ran with: {}", kern_return);
    }
    Ok(())
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
    let state = thread_state64_t::new();
    // let state_count = thread_state64_t::count();
    //
    // let kernel_return = unsafe {
    //     thread_act::thread_get_state(
    //         port,
    //         THREAD_STATE64,
    //         mem::transmute(&state),
    //         mem::transmute(&state_count),
    //     )
    // };
    //
    // println!("{}", kernel_return);
    // println!("{:?}", state);

    Ok(state)
}
