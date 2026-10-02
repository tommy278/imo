use crate::cli::handle_user_debugger_menu;
use crate::error::DebuggerError;
use crate::session;
use crate::sys::macos::error::MacOSError;
use crate::sys::SystemError;

use mach2::{kern_return, mach_types, traps};
use rustyline::DefaultEditor;
use std::process;

pub fn debug(rl: &mut DefaultEditor, binary_path: &str) -> Result<(), DebuggerError> {
    let child = process::Command::new(binary_path)
        .spawn()
        .map_err(|err| MacOSError::Command(err))?;
    let pid = child.id() as i32;

    let mut task_port: mach_types::task_t = 0;

    let kern_return =
        unsafe { traps::task_for_pid(traps::mach_task_self(), pid, task_port as *mut u32) };

    if kern_return == kern_return::KERN_SUCCESS {
        let mut session = session::DebugSession::new(task_port, binary_path)?;
        handle_user_debugger_menu(&mut session, rl);
    }
    Ok(())
}
