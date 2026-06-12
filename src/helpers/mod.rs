pub mod dwarf;

use std::{
    collections::HashSet,
    io::{self, Write},
    path::Path,
};

use crate::{
    interface::linux::BreakPoint,
    session::{BreakpointTarget, DebugSession},
};

/// Display an interactive menu at breakpoints
pub fn handle_user_debugger_menu(session: &mut DebugSession) {
    // Flush so the print statement is immediately displayed on screen
    let flush_output = || {
        io::stdout().flush().unwrap();
    };

    let mut buffer = String::new();

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

                let create_breakpoint = |absolute_address: u64, session: &mut DebugSession| {
                    let mut breakpoint = BreakPoint::new(absolute_address);
                    breakpoint.enable(session.pid);
                    session
                        .active_breakpoints
                        .insert(absolute_address, breakpoint);
                };

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

fn create_breakpoint(absolute_address: u64, session: &mut DebugSession) {
    let mut breakpoint = BreakPoint::new(absolute_address);
    breakpoint.enable(session.pid);
    session
        .active_breakpoints
        .insert(absolute_address, breakpoint);
}

// TODO: Change breakpoint target to be more os generic
fn handle_line_index_result(session: &mut DebugSession, line_index: &[BreakpointTarget]) {
    if line_index.is_empty() {
        println!("unimplemented!");
        return;
    }

    if line_index.len() == 1 {
        // Safe unwrap, guarnteed to be an item with len being 1
        let bp = line_index.first().unwrap();
        let absolute_address = session.get_absolute_address(bp.relative_address);
        create_breakpoint(absolute_address, session);
        return;
    }

    println!("These are your options: {:?}", line_index);

    // Listen for user's choice for path
    let mut index: usize = 0;
    loop {
        let buffer = io::read_to_string(io::stdin()).expect("Could not read input");
        if let Ok(idx) = buffer.parse::<usize>() {
            if idx >= line_index.len() {
                println!("{} is not a valid option", idx);
                continue;
            }

            index = idx;
            break;
        }
    }

    // Finally resolve the conflicting files
    // Safe unwrap, index is valid from the guarded input
    let bp = line_index.iter().nth(index).unwrap();
    let absolute_address = session.get_absolute_address(bp.relative_address);
    create_breakpoint(absolute_address, session);
}
