use std::{
    io::{self, Write},
    path::Path,
};

use rustc_hash::FxHashSet;

use crate::helpers::trim_file_path;
use crate::session::{DebugSession, interface};

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
    F: FnOnce(&mut DebugSession, usize) -> interface::BreakpointMutationResult,
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
        interface::BreakpointMutationResult::Updated => {
            // Format action into being past tense
            println!("Successfully {}d breakpoint {}", action, user_index);
        }
        interface::BreakpointMutationResult::AlreadyInState => {
            println!("Breakpoint {} is already {}d", user_index, action);
        }
        interface::BreakpointMutationResult::NotFound => {
            println!("Could not {} breakpoint", action);
        }
    }
}

pub fn handle_breakpoint_clearing(
    session: &mut DebugSession,
    line_number: u64,
    file: Option<&str>,
) {
    let bp = session.clear_breakpoint(line_number, file);
    println!("{:?}", bp);
}

pub fn handle_breakpoint_setting(
    session: &mut DebugSession,
    line_index: &[interface::BreakpointTarget],
    line_number: u64,
) {
    if line_index.is_empty() {
        panic!("Error occured creating session")
    }

    let unique_files: FxHashSet<&Path> = line_index.iter().map(|bp| bp.file.as_ref()).collect();

    // All addresses belong to the same file
    if unique_files.len() == 1 {
        // Input the first file name since they are all the same
        let default = line_index[0].file.as_ref();
        let (bp_for_line, first_bp) = session.create_breakpoint(line_number, default);

        handle_break_metadata(session, bp_for_line, first_bp, line_number);
        return;
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

    let (bp_for_line, first_bp) = session.create_breakpoint(line_number, chosen_file);
    handle_break_metadata(session, bp_for_line, first_bp, line_number);
}

/// Create breakpoints and give user the data regarding them
fn handle_break_metadata(
    session: &mut DebugSession,
    bp_for_line: u64,
    first_bp: interface::BreakpointTarget,
    line_number: u64,
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
