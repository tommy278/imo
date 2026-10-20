use crate::dwarf::error::CacheSetupError;
use crate::sys::macos::error::MacOSError;
use crate::sys::macos::{MessageReply, PlatformRegStruct, ProcessHandle};

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

#[cfg(target_arch = "x86_64")]
use mach2::thread_status::x86_DEBUG_STATE64 as DEBUG_STATE64;
#[cfg(target_arch = "aarch64")]
use mach2::thread_status::ARM_DEBUG_STATE64 as DEBUG_STATE64;

pub const TRAP_FLAG: u64 = 1u64 << 8;

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
            // Populate the base address with the first memory region found;
            if session.base_address == 0 {
                session.base_address = address;
            }

            // Extract permissions and populate the whole process memory region
            let is_readable = info.protection & mach2::vm_prot::VM_PROT_READ == 0;
            let is_writable = info.protection & mach2::vm_prot::VM_PROT_WRITE == 0;
            let is_executable = info.protection & mach2::vm_prot::VM_PROT_EXECUTE == 0;

            session.process_map.ranges.push(crate::sys::MemoryRegion {
                start_address: address,
                end_address: address + size,
                is_readable,
                is_writable,
                is_executable,
            });
            address += size;
        }
    }

    Ok(())
}

pub fn read_bytes(port: ProcessHandle, ptr: usize, len: usize) -> Result<Vec<u8>, MacOSError> {
    let mut buffer = vec![0u8; len];
    let mut out_size = 0;

    let mut kern_return = unsafe {
        vm::mach_vm_read_overwrite(
            port,
            ptr as u64,
            len as u64,
            buffer.as_mut_ptr() as u64,
            &mut out_size,
        )
    };

    if kern_return != mach2::kern_return::KERN_SUCCESS {
        println!("READ FAILED");
    }

    Ok(buffer)
}

pub fn step(
    port: ProcessHandle,
    thread: u32,
    mut message: crate::sys::os::MessageReply,
) -> Result<(), MacOSError> {
    let mut regs = get_regs(thread)?;
    regs.__rflags |= TRAP_FLAG;

    let mut kern_return = unsafe {
        mach2::thread_act::thread_set_state(
            thread,
            THREAD_STATE64,
            &mut regs as *mut thread_state64_t as *mut u32,
            thread_state64_t::count(),
        )
    };

    if kern_return != mach2::kern_return::KERN_SUCCESS {
        println!("Failed to update registers");
    }

    kern_return = unsafe {
        mach2::message::mach_msg(
            &mut message as *mut MessageReply as *mut mach2::message::mach_msg_header_t,
            mach2::message::MACH_SEND_MSG,
            std::mem::size_of::<MessageReply>() as u32,
            0,
            mach2::port::MACH_PORT_NULL,
            mach2::message::MACH_MSG_TIMEOUT_NONE,
            mach2::port::MACH_PORT_NULL,
        )
    };

    if kern_return != mach2::kern_return::KERN_SUCCESS {
        println!("Could not revive thread: {}", kern_return);
    }

    kern_return = unsafe { mach2::task::task_resume(port) };

    if kern_return != mach2::kern_return::KERN_SUCCESS {
        println!("Failed to resume task: {}", kern_return);
    }

    Ok(())
}

pub fn continue_session(port: ProcessHandle) -> Result<(), MacOSError> {
    unsafe {
        let kern_return = mach2::task::task_resume(port);

        if kern_return != mach2::kern_return::KERN_SUCCESS {
            todo!()
        }
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

pub fn write_data(port: ProcessHandle, address: u64, data: &[u8]) -> Result<(), MacOSError> {
    unsafe {
        let mut kern_return = mach2::vm::mach_vm_protect(
            port,
            address,
            1,
            0,
            mach2::vm_prot::VM_PROT_READ
                | mach2::vm_prot::VM_PROT_COPY
                | mach2::vm_prot::VM_PROT_WRITE,
        );

        if kern_return != mach2::kern_return::KERN_SUCCESS {
            eprintln!("Could not protect address : {}", kern_return);
        }

        kern_return = mach2::vm::mach_vm_write(port, address, data.as_ptr() as usize, 1);

        if kern_return != mach2::kern_return::KERN_SUCCESS {
            eprintln!("Could not write to address : {}", kern_return);
        }

        // Revert permissions back to default
        kern_return = mach2::vm::mach_vm_protect(
            port,
            address,
            1,
            0,
            mach2::vm_prot::VM_PROT_EXECUTE | mach2::vm_prot::VM_PROT_READ,
        );

        if kern_return != mach2::kern_return::KERN_SUCCESS {
            eprintln!("Could not protect address : {}", kern_return);
        }
    }

    Ok(())
}

pub fn get_regs(thread: u32) -> Result<PlatformRegStruct, MacOSError> {
    let mut state = thread_state64_t::new();
    let mut state_count = thread_state64_t::count();

    let kern_return = unsafe {
        thread_act::thread_get_state(
            thread,
            THREAD_STATE64,
            std::mem::transmute(&state),
            &mut state_count,
        )
    };

    if kern_return != mach2::kern_return::KERN_SUCCESS {
        // TODO: Handle this case: process probaably did not start yet
        println!("Failed to get the reg state: {}", kern_return);
    }

    Ok(state)
}

// pub fn get_regs_for_all_threads(port: ProcessHandle) -> Result<PlatformRegStruct, MacOSError> {
//     let mut raw_thread_list: mach2::mach_types::thread_act_port_array_t = std::ptr::null_mut();
//     let mut count = 0;
//
//     let mut kern_return =
//         unsafe { mach2::task::task_threads(port, &mut raw_thread_list, &mut count) };
//
//     if kern_return != mach2::kern_return::KERN_SUCCESS {
//         eprintln!("Error fetching the thread list");
//     }
//
//     let thread_list = unsafe { std::slice::from_raw_parts(raw_thread_list, count as usize) };
//
//     for thread in thread_list {
//         let state = thread_state64_t::new();
//         let state_count = thread_state64_t::count();
//
//         kern_return = unsafe {
//             thread_act::thread_get_state(
//                 *thread,
//                 THREAD_STATE64,
//                 mem::transmute(&state),
//                 mem::transmute(&state_count),
//             )
//         };
//
//         println!("{:?}", state);
//     }
//
//     todo!()
// }
