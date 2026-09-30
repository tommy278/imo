use crate::sys::SystemError;

use mach2::{kern_return, mach_types, traps};
use rustyline::DefaultEditor;
use std::process;

pub fn debug(_rl: &mut DefaultEditor, binary_path: &str) -> Result<(), SystemError> {
    let child = process::Command::new(binary_path).spawn().unwrap();
    let pid = child.id() as i32;

    let mut task_port: mach_types::task_t = 0;

    let kern_return =
        unsafe { traps::task_for_pid(traps::mach_task_self(), pid, task_port as *mut u32) };

    let exe_path = std::env::current_exe().unwrap();

    println!("{:?}", (kern_return, kern_return::KERN_FAILURE));
    Ok(())
}
