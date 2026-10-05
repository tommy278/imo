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

    let mut spawn_attr: libc::posix_spawnattr_t = unsafe { std::ptr::null_mut() };
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

    let mut task_port: mach2::port::mach_port_name_t = 0;

    let kern_return = unsafe { traps::task_for_pid(traps::mach_task_self(), pid, &mut task_port) };

    println!("Port: {} and Kern return: {}", task_port, kern_return);

    if kern_return == kern_return::KERN_SUCCESS {
        let mut session = session::DebugSession::new(task_port, binary_path)?;
        let mut exception_port: mach2::port::mach_port_name_t = 0;
        unsafe {
            let kern_retun = mach2::mach_port::mach_port_allocate(
                traps::mach_task_self(),
                mach2::port::MACH_PORT_RIGHT_RECEIVE,
                &mut exception_port,
            );

            if kern_return != kern_return::KERN_SUCCESS {
                eprintln!("Failed to create exception port")
            }

            let kern_return = mach2::mach_port::mach_port_insert_right(
                traps::mach_task_self(),
                exception_port,
                exception_port,
                mach2::message::MACH_MSG_TYPE_MAKE_SEND,
            );

            if kern_return != kern_return::KERN_SUCCESS {
                eprintln!("Failed to insert exception port")
            }

            let kern_return = mach2::task::task_set_exception_ports(
                traps::mach_task_self(),
                mach2::exception_types::EXC_BREAKPOINT,
                exception_port,
                mach2::exception_types::EXCEPTION_DEFAULT as i32,
                THREAD_STATE64,
            );

            if kern_return != kern_return::KERN_SUCCESS {
                println!("KERN RETURN: {}", kern_return);
                eprintln!("Failed to set exception port")
            }

            let kern_return = mach2::task::task_resume(task_port);

            if kern_return != kern_return::KERN_SUCCESS {
                eprintln!("Failed to resume process");
            }

            println!("{}", exception_port);
        }
        handle_user_debugger_menu(&mut session, rl);
    }
    Ok(())
}
