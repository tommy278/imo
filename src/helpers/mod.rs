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
    for index in session.line_index.iter() {
        println!("{:?}", index);
    }
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
                    handle_line_index_result(session, &line_index);
                }

                // Handle setting breakpoint if only line_number is provided
                // eg: break 12
                if let Ok(line_number) = arg.parse::<u64>() {
                    let line_index = session.get_breakpoint_target(line_number).unwrap();
                    handle_line_index_result(session, &line_index);
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

fn handle_line_index_result(session: &mut DebugSession, line_index: &[BreakpointTarget]) {
    // TODO: Handle case of empty line index
    // Program shouldve crashed by now
    if line_index.is_empty() {
        println!("unimplemented!");
        return;
    }

    let unique_files: FxHashSet<&Path> = line_index.iter().map(|bp| bp.file.as_ref()).collect();

    // All addresses belong to the same file
    if unique_files.len() == 1 {
        handle_break_metadata(session, line_index);
        return;
    }

    let file_choices: Vec<&Path> = unique_files.into_iter().collect();

    println!("Ambigous line number");
    for (idx, file) in file_choices.iter().enumerate() {
        println!("[{}] {}", idx, file.display());
    }
    print!("Please choose a file: ");
    flush_output();

    // Listen for user's choice for path
    let mut index: usize;
    let mut buffer = String::new();

    loop {
        buffer.clear();
        io::stdin()
            .read_line(&mut buffer)
            .expect("Could not read input");

        if let Ok(idx) = buffer.trim().parse::<usize>() {
            if idx >= file_choices.len() {
                println!("{} is not a valid option", idx);
                continue;
            }

            index = idx;
            break;
        }
    }

    let chosen_file = file_choices[index];

    let chosen_line_index: Vec<BreakpointTarget> = (line_index)
        .to_vec()
        .into_iter()
        .filter(|bp| bp.file.as_ref() == chosen_file)
        .collect::<Vec<_>>();

    handle_break_metadata(session, &chosen_line_index);
    return;
}

fn handle_break_metadata(session: &mut DebugSession, line_index: &[BreakpointTarget]) {
    let mut bp_for_line = 0;
    for bp in line_index {
        session.create_breakpoint(bp.relative_address);
        bp_for_line += 1;
    }
    let bp_count = session.increase_breakpoint_count();

    let first_bp_relative_address = line_index[0].relative_address;

    let line_number = session
        .get_info_from_address(first_bp_relative_address)
        .unwrap()
        .line;

    let location_detail = if bp_for_line == 1 {
        format!("line {}", line_number)
    } else {
        format!("({} locations)", bp_for_line)
    };

    println!(
        "Breakpoint {} at 0x{:X}: file {}, {}",
        bp_count,
        first_bp_relative_address,
        line_index[0].file.display(),
        location_detail
    );

    println!("{:?}", line_index);
}
