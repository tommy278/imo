pub mod dwarf;

use std::{
    io::{self, Write},
    path::Path,
};

use rustc_hash::FxHashSet;

use crate::session::{BreakpointTarget, DebugSession};

/// Flush so the print statement is immediately displayed on screen
/// Used for print statement since its not flushed automcatically unlike println
fn flush_output() {
    io::stdout().flush().unwrap();
}

/// Display an interactive menu at breakpoints
pub fn handle_user_debugger_menu(session: &mut DebugSession) {
    let mut buffer = String::new();
    // For debugging purposes
    loop {
        print!("(imo) ");
        flush_output();

        // Clear previous data that could corrupt input
        buffer.clear();

        io::stdin()
            .read_line(&mut buffer)
            .expect("Could not read line");

        let input = buffer.trim().to_lowercase();
        let mut parts = input.split_whitespace();

        match parts.next().unwrap() {
            "run" | "c" | "continue" => {
                session.continue_session();
                break;
            }
            "b" | "break" => {
                let arg = parts.next().expect("Did not provide a second argument");

                // Handle setting breakpoint if filename and line_number are provided
                // eg: break running_task:6
                if let Some((file_name, line_number)) = arg.split_once(":") {
                    let line_number = line_number.parse::<u64>().expect("Could not parse number");

                    let line_index = session.get_specific_breakpoint_target(file_name, line_number);
                    handle_breakpoint_setting(session, &line_index, line_number);
                }

                // Handle setting breakpoint if only line_number is provided
                // eg: break 12
                if let Ok(line_number) = arg.parse::<u64>() {
                    let line_index = session.get_breakpoint_target(line_number).unwrap();
                    handle_breakpoint_setting(session, &line_index, line_number);
                }
            }
            "clear" => {
                let arg = parts.next().expect("Did not provide a second argument");

                // Handle clearing breakpoint if filename and line_number are provided
                // eg: clear running_task:6
                if let Some((file_name, line_number)) = arg.split_once(":") {
                    let line_number = line_number.parse::<u64>().expect("Could not parse number");

                    let line_index = session.get_specific_breakpoint_target(file_name, line_number);
                    handle_breakpoint_clearing(session, line_number, Some(file_name));
                }

                // Handle clearing breakpoint if only line_number is provided
                // eg: clear 12
                if let Ok(line_number) = arg.parse::<u64>() {
                    let line_index = session.get_breakpoint_target(line_number).unwrap();
                    handle_breakpoint_clearing(session, line_number, None);
                }
            }
            "debug" => {
                for entry in session.breakpoint_index_tracker.iter() {
                    println!("{:?}", entry);
                }
            }
            "q" | "quit" => {
                // End current debug session
                session.kill_session();
                break;
            }
            _ => {
                println!("Not handled yet");
            }
        }
    }
}

fn handle_breakpoint_clearing(session: &mut DebugSession, line_number: u64, file: Option<&str>) {
    let bp = session.clear_breakpoint(line_number, file);
    println!("{:?}", bp);
}

fn handle_breakpoint_setting(
    session: &mut DebugSession,
    line_index: &[BreakpointTarget],
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
    return;
}

/// Create breakpoints and give user the data regarding them
fn handle_break_metadata(
    session: &mut DebugSession,
    bp_for_line: u64,
    first_bp: BreakpointTarget,
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

/// Trim file path for the main source code
fn trim_file_path(path: &Path) -> String {
    let path = path.display().to_string();
    let res = path.split("/").last().unwrap_or("main").to_owned();
    res
}
