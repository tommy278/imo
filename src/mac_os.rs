use crate::cli::handle_user_debugger_menu;
use crate::error::DebuggerError;
use crate::session;
use crate::sys::macos::error::MacOSError;

use mach2::{kern_return, mach_types, traps};
use rustyline::DefaultEditor;

#[cfg(target_arch = "x86_64")]
use mach2::thread_status::x86_THREAD_STATE64 as THREAD_STATE64;
#[cfg(target_arch = "aarch64")]
use mach2::thread_status::ARM_THREAD_STATE64 as THREAD_STATE64;

pub fn debug(rl: &mut DefaultEditor, binary_path: &str) -> Result<(), DebuggerError> {
    let mut pid: i32 = 0;
    let path = std::ffi::CString::new(binary_path).map_err(|e| MacOSError::CString(e))?;

    let mut spawn_attr: libc::posix_spawnattr_t = std::ptr::null_mut();
    let return_value = unsafe { libc::posix_spawnattr_init(&mut spawn_attr) };

    if return_value != 0 {
        eprintln!("Error creating attr");
        return Ok(());
    }

    let flag = libc::POSIX_SPAWN_START_SUSPENDED as i16;
    let return_value = unsafe { libc::posix_spawnattr_setflags(&mut spawn_attr, flag) };

    if return_value != 0 {
        eprintln!("Error creating flag");
        return Ok(());
    }

    let return_value = unsafe {
        libc::posix_spawn(
            &mut pid,
            path.as_ptr(),
            std::ptr::null(),
            &mut spawn_attr,
            std::ptr::null(),
            std::ptr::null(),
        )
    };

    if return_value != 0 {
        eprintln!("Error creating process");
        return Ok(());
    }

    let mut child_task_port: mach2::port::mach_port_name_t = 0;

    let kern_return =
        unsafe { traps::task_for_pid(traps::mach_task_self(), pid, &mut child_task_port) };

    println!("Port: {} and Kern return: {}", child_task_port, kern_return);

    if kern_return == kern_return::KERN_SUCCESS {
        let mut session = session::DebugSession::new(child_task_port, binary_path)?;
        let mut exception_port: mach2::port::mach_port_name_t = 0;
        unsafe {
            let mut child_name: mach2::port::mach_port_name_t = 0;

            // Acquire child name
            let kern_return = mach2::mach_port::mach_port_allocate(
                child_task_port,
                mach2::port::MACH_PORT_RIGHT_RECEIVE,
                &mut child_name,
            );

            if kern_return != kern_return::KERN_SUCCESS {
                eprintln!("Failed to create child name");
            }

            // Destroy temporary placeholder
            let kern_return = mach2::mach_port::mach_port_mod_refs(
                child_task_port,
                child_name,
                mach2::port::MACH_PORT_RIGHT_RECEIVE,
                -1,
            );

            if kern_return != kern_return::KERN_SUCCESS {
                eprintln!("Failed to delete child");
            }

            // Allocate the exception port
            let kern_retun = mach2::mach_port::mach_port_allocate(
                traps::mach_task_self(),
                mach2::port::MACH_PORT_RIGHT_RECEIVE,
                &mut exception_port,
            );

            if kern_return != kern_return::KERN_SUCCESS {
                eprintln!("Failed to create exception port")
            }

            let kern_return = mach2::mach_port::mach_port_insert_right(
                child_task_port,
                child_name,
                exception_port,
                mach2::message::MACH_MSG_TYPE_MAKE_SEND,
            );

            if kern_return != kern_return::KERN_SUCCESS {
                eprintln!("Failed to insert exception port")
            }

            // Set specific exceptions to be tracked
            let kern_return = mach2::task::task_set_exception_ports(
                child_task_port,
                mach2::exception_types::EXC_BREAKPOINT,
                child_name,
                mach2::exception_types::EXCEPTION_DEFAULT as i32,
                THREAD_STATE64,
            );

            if kern_return != kern_return::KERN_SUCCESS {
                println!("KERN RETURN: {}", kern_return);
                eprintln!("Failed to set exception port")
            }
        }

        handle_user_debugger_menu(&mut session, rl)?;

        let mut msg_header = mach2::message::mach_msg_header_t::default();
        let option = mach2::message::MACH_RCV_MSG;
        let size = 0;
        let recieve = 100;
        let timeout = mach2::message::MACH_MSG_TIMEOUT_NONE;
        let notify = mach2::port::MACH_PORT_NULL;

        loop {
            let message = unsafe {
                mach2::message::mach_msg(
                    &mut msg_header,
                    option,
                    size,
                    recieve,
                    exception_port,
                    timeout,
                    notify,
                )
            };

            println!("This is the message: {:?}", message);
        }
    }
    Ok(())
}
