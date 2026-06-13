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
                    handle_line_index_result(session, &line_index, line_number);
                }

                // Handle setting breakpoint if only line_number is provided
                // eg: break 12
                if let Ok(line_number) = arg.parse::<u64>() {
                    let line_index = session.get_breakpoint_target(line_number).unwrap();
                    handle_line_index_result(session, &line_index, line_number);
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

fn handle_line_index_result(
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
        handle_break_metadata(session, line_index, line_number);
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

    // Filter out files that do not match the user chosen_file
    // Pass this filtered vec into the handle_break_metadata for data display
    let chosen_line_index: Vec<BreakpointTarget> = (line_index)
        .to_vec()
        .into_iter()
        .filter(|bp| bp.file.as_ref() == chosen_file)
        .collect::<Vec<_>>();

    handle_break_metadata(session, &chosen_line_index, line_number);
    return;
}

/// Create breakpoints and give user the data regarding them
fn handle_break_metadata(
    session: &mut DebugSession,
    line_index: &[BreakpointTarget],
    line_number: u64,
) {
    // Keep track of how many breakpoints were created for this line
    // A singular line can have multiple breakpoints
    let mut bp_for_line = 0;
    for bp in line_index {
        session.create_breakpoint(bp.relative_address);
        bp_for_line += 1;
    }

    // Update the total breakpoint count for the session
    let bp_count = session.increase_breakpoint_count();

    // Keep track of the first file in the line index (display regardless of how many files)
    // Safe index, at this point guranteed to exist due to previous checks in handle_line_index_result
    let first_bp_relative_address = line_index[0].relative_address;
    let trimmed_path = trim_file_path(line_index[0].file.as_ref());

    let location_detail = if bp_for_line == 1 {
        format!("line {}", line_number)
    } else {
        format!("({} locations)", bp_for_line)
    };

    println!(
        "Breakpoint {} at 0x{:X}: file {}, {}",
        bp_count, first_bp_relative_address, trimmed_path, location_detail
    );
}

/// Trim file path for the main source code
fn trim_file_path(path: &Path) -> String {
    let path = path.display().to_string();
    let res = path.split("/").last().unwrap_or("main").to_owned();
    res
}
