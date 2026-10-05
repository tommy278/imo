use crate::cli::handle_user_debugger_menu;
use crate::error::DebuggerError;
use crate::session;
use crate::sys::macos::error::MacOSError;

use mach2::{kern_return, mach_types, traps};
use rustyline::DefaultEditor;

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

    let kern_return =
        unsafe { traps::task_for_pid(traps::mach_task_self(), pid, &mut task_port as *mut _) };

    println!("Port: {} and Kern return: {}", task_port, kern_return);

    if kern_return == kern_return::KERN_SUCCESS {
        let mut session = session::DebugSession::new(task_port, binary_path)?;
        unsafe {
            libc::kill(pid, libc::SIGCONT);
        }
        handle_user_debugger_menu(&mut session, rl);
    }
    Ok(())
}
