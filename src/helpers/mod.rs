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

                    let targets = session.get_specific_breakpoint_target(file_name, line_number);

                    println!("{:?}", targets);
                    let absolute_addresses: Vec<u64> = targets
                        .iter()
                        .map(|bp| session.get_absolute_address(bp.relative_address))
                        .collect();

                    for absolute_address in absolute_addresses {
                        create_breakpoint(absolute_address, session);
                    }
                }

                if let Ok(line_number) = arg.parse::<u64>() {
                    let line_index = session.get_breakpoint_target(line_number).unwrap();
                    println!("{:?}", line_index);

                    let all_files = get_all_files(line_index);

                    if all_files.len() == 1 {
                        // This is a safe unwrap, the len is one there is guranteeed to be an item
                        let bp = line_index.first().unwrap();

                        // Proceed with setting the breakpoint as normal
                        let absolute_address = session.get_absolute_address(bp.relative_address);
                        create_breakpoint(absolute_address, session);
                    } else {
                        println!("{:?}", all_files);
                    }
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

fn get_all_files(line_index: &[BreakpointTarget]) -> HashSet<&Path> {
    // Using a hashset because there can be multiple addresses of the same file on the same line
    let mut files = HashSet::new();
    for index in line_index {
        files.insert(index.file.as_ref());
    }
    files
}
