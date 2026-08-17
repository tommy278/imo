use std::{
    io::{self, Write},
    path::Path,
};

use rustc_hash::FxHashSet;

use crate::session::{DebugSession, interface};
use crate::sys::SystemError;
use crate::utils::trim_file_path;

/// Flush so the print statement is immediately displayed on screen
/// Used for print statement since its not flushed automcatically unlike println
pub fn flush_output() {
    io::stdout().flush().unwrap();
}

pub fn handle_event_by_index<F>(
    session: &mut DebugSession,
    user_index: usize,
    func: F,
    action: &str,
) where
    F: FnOnce(&mut DebugSession, usize) -> Result<interface::BreakpointMutationResult, SystemError>,
{
    // Index 0 never exists
    // Index starts at 1
    if user_index == 0 {
        println!("No such index");
        return;
    }

    // Get the actual 0 based index once safe
    let vec_index = user_index - 1;

    // Index out of bounds or already deleted
    if vec_index >= session.current_index() {
        println!("No such index");
        return;
    }

    match func(session, vec_index) {
        Ok(interface::BreakpointMutationResult::Updated) => {
            // Format action into being past tense
            println!("Successfully {}d breakpoint {}", action, user_index);
        }
        Ok(interface::BreakpointMutationResult::AlreadyInState) => {
            println!("Breakpoint {} is already {}d", user_index, action);
        }
        Ok(interface::BreakpointMutationResult::NotFound) => {
            println!("Could not {} breakpoint", action);
        }
        Err(e) => eprintln!("{e}"),

        // This is for creation which is not index based
        _ => unreachable!(),
    }
}

pub fn handle_breakpoint_clearing(
    session: &mut DebugSession,
    line_number: u32,
    file: Option<&str>,
) {
    match session.clear_breakpoint(line_number, file) {
        Ok(bp) => {
            if bp.is_empty() {
                println!("No breakpoint cleared");
                return;
            }
            println!("{:?}", bp)
        }
        Err(e) => eprintln!("{e}"),
    }
}

pub fn handle_breakpoint_setting(
    session: &mut DebugSession,
    line_index: &[interface::BreakpointTarget],
    line_number: u32,
) {
    if line_index.is_empty() {
        println!("Cannot set breakpoint at target");
        return;
    }

    let unique_files: FxHashSet<&Path> = line_index
        .iter()
        .map(|bp| bp.file.as_ref())
        .filter(|file| {
            let path = file.to_string_lossy();
            !path.contains("/rustc/") && !path.contains("/rust/deps")
        })
        .collect();

    if unique_files.is_empty() {
        println!("Cannot set breakpoint at target");
        return;
    }

    // All addresses belong to the same file
    if unique_files.len() == 1 {
        // Input the first file name since they are all the same
        let default = line_index[0].file.as_ref();
        let result = session.create_breakpoint(line_number, default);

        match result {
            Ok(interface::BreakpointMutationResult::Created { count, target }) => {
                handle_break_metadata(session, count, target, line_number);
                return;
            }
            Ok(interface::BreakpointMutationResult::NotFound) => {
                println!("Could not find breakpoint")
            }
            Err(e) => eprintln!("{e}"),
            _ => unreachable!(),
        }
    }

    // All addresses are not the same file
    // The user has to specifically pick the file they want
    let file_choices: Vec<&Path> = unique_files.into_iter().collect();

    println!("Ambigous line number");
    for (idx, file) in file_choices.iter().enumerate() {
        println!("[{}] {}", idx, file.display());
    }
    print!("Please choose a file: ");
    flush_output();

    // Listen for user's choice for path
    let mut buffer = String::new();

    // Record user's choice in index
    let index: usize = loop {
        buffer.clear();
        io::stdin()
            .read_line(&mut buffer)
            .expect("Could not read input");

        if let Ok(idx) = buffer.trim().parse::<usize>() {
            if idx >= file_choices.len() {
                println!("{} is not a valid option", idx);
                continue;
            }

            break idx;
        }
    };

    let chosen_file = file_choices[index];

    let result = session.create_breakpoint(line_number, chosen_file);
    match result {
        Ok(interface::BreakpointMutationResult::Created { count, target }) => {
            handle_break_metadata(session, count, target, line_number);
            return;
        }
        Ok(interface::BreakpointMutationResult::NotFound) => println!("Could not find breakpoint"),
        Err(e) => eprintln!("{e}"),
        _ => unreachable!(),
    }
}

/// Create breakpoints and give user the data regarding them
fn handle_break_metadata(
    session: &mut DebugSession,
    bp_for_line: u8,
    first_bp: interface::BreakpointTarget,
    line_number: u32,
) {
    // Handle metadata correctly
    let first_bp_relative_address = first_bp.relative_address;
    let trimmed_path = trim_file_path(first_bp.file.as_ref());

    let location_detail = if bp_for_line == 1 {
        format!("line {}", line_number)
    } else {
        format!("({} locations)", bp_for_line)
    };

    println!(
        "Breakpoint {} at 0x{:X}: file {}, {}",
        session.current_index(),
        first_bp_relative_address,
        trimmed_path,
        location_detail
    );
}

#[macro_export]
macro_rules! handle_cmd {
    ($session:expr, $cmd_call:expr) => {
        if $session.is_idle() {
            println!("Session not started yet");
        } else {
            if let Err(err) = $cmd_call {
                println!("{err}");
            }
            return Ok(());
        }
    };
}

#[macro_export]
macro_rules! parse_line_arg {
    ($line_str:expr, $target_type:ty) => {
        match $line_str.parse::<$target_type>() {
            Ok(num) => num,
            Err(_) => {
                println!(
                    "Error: '{}' is not a valid {} value",
                    $line_str,
                    stringify!($target_type)
                );
                continue;
            }
        }
    };
}

#[macro_export]
macro_rules! parse_arg {
    ($parts:expr, $cmd: expr) => {
        match $parts.next() {
            Some(arg) => arg,
            None => {
                eprintln!("Error: '{}' requires a location", $cmd);
                continue;
            }
        }
    };
}

pub use crate::handle_cmd;
pub use crate::parse_arg;
pub use crate::parse_line_arg;
