pub mod dwarf;

use std::io::{self, Write};

use crate::{interface::linux::BreakPoint, session::DebugSession};

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
            "break" => {
                // TODO: Include compatibility with specic file name eg break file.c:24
                if let Some(num_str) = parts.next() {
                    if let Ok(line_number) = num_str.parse::<u64>() {
                        let address = session.get_breakpoint_target(line_number).unwrap();
                        let absolute_address =
                            session.get_absolute_address(address.relative_address);

                        let mut breakpoint = BreakPoint::new(absolute_address);
                        breakpoint.enable(session.pid);
                        session
                            .active_breakpoints
                            .insert(absolute_address, breakpoint);
                    }
                }
            }
            "n" | "no" => {
                // TODO: Find the idiomatic way to continue from here
                session.kill_session();
                break;
            }
            _ => {
                println!("Not handled yet");
            }
        }
    }
}
