pub mod utils;

use rustyline::error::ReadlineError;
use rustyline::{DefaultEditor, Result};

use crate::session::DebugSession;
use utils::{
    handle_breakpoint_clearing, handle_breakpoint_setting, handle_cmd, handle_event_by_index,
    parse_arg, parse_line_arg,
};

/// Display an interactive menu at breakpoints
pub fn handle_user_debugger_menu(session: &mut DebugSession, rl: &mut DefaultEditor) -> Result<()> {
    loop {
        let read_line = rl.readline("(imo) ");

        match read_line {
            Ok(line) => {
                rl.add_history_entry(line.as_str())?;

                let mut parts = line.split_whitespace();

                let Some(part) = parts.next() else {
                    continue;
                };

                match part {
                    "run" => {
                        if session.is_idle() {
                            session.toggle_running();
                            if let Err(err) = session.continue_session() {
                                println!("{err}")
                            }
                            return Ok(());
                        } else {
                            println!("Session is already running")
                        }
                    }
                    "c" | "continue" => {
                        if !session.is_idle() {
                            session.toggle_running();
                            if let Err(err) = session.continue_session() {
                                println!("{err}")
                            }
                            return Ok(());
                        } else {
                            println!("Session is already running")
                        }
                    }
                    // Step a single instruction
                    "si" | "stepi" => handle_cmd!(session, session.complete_single_step()),
                    // Step into
                    "s" | "step" => handle_cmd!(session, session.begin_step_into()),
                    // Step Over / Next
                    "n" | "next" => handle_cmd!(session, session.begin_step_over()),

                    "f" | "fin" | "finish" => handle_cmd!(session, session.begin_finish()),

                    "b" | "break" => {
                        let arg = parse_arg!(parts, "break");

                        // Handle setting breakpoint if filename and line_number are provided
                        // eg: break running_task:6
                        if let Some((file_name, line_num)) = arg.split_once(":") {
                            let line_number = parse_line_arg!(line_num, u32);

                            let line_index =
                                session.get_specific_breakpoint_target(file_name, line_number);
                            handle_breakpoint_setting(session, &line_index, line_number);
                            continue;
                        }

                        // Handle setting breakpoint if only line_number is provided
                        // eg: break 12

                        let line_number = parse_line_arg!(arg, u32);
                        let Some(line_index) = session.get_breakpoint_target(line_number) else {
                            eprintln!("Cannot set breakpoint at target");
                            continue;
                        };
                        handle_breakpoint_setting(session, &line_index, line_number);
                    }
                    "clear" => {
                        let arg = parse_arg!(parts, "clear");

                        // Handle clearing breakpoint if filename and line_number are provided
                        // eg: clear running_task:6
                        if let Some((file_name, line_number)) = arg.split_once(":") {
                            let line_number = parse_line_arg!(line_number, u32);
                            handle_breakpoint_clearing(session, line_number, Some(file_name));
                            continue;
                        }

                        // Handle clearing breakpoint if only line_number is provided
                        // eg: clear 12

                        let line_number = parse_line_arg!(arg, u32);
                        handle_breakpoint_clearing(session, line_number, None);
                    }
                    "d" | "delete" => {
                        let arg = parse_arg!(parts, "delete");

                        let user_index = parse_line_arg!(arg, usize);
                        handle_event_by_index(
                            session,
                            user_index,
                            |s, idx| s.delete_breakpoint(idx),
                            "delete",
                        );
                    }
                    "e" | "enable" => {
                        let arg = parse_arg!(parts, "enable");

                        let user_index = parse_line_arg!(arg, usize);
                        handle_event_by_index(
                            session,
                            user_index,
                            |s, idx| s.enable_breakpoint(idx),
                            "enable",
                        );
                    }
                    "dis" | "disable" => {
                        let arg = parse_arg!(parts, "disable");

                        let user_index = parse_line_arg!(arg, usize);
                        handle_event_by_index(
                            session,
                            user_index,
                            |s, idx| s.disable_breakpoint(idx),
                            "disable",
                        );
                    }
                    "i" | "info" => {
                        let arg = parse_arg!(parts, "info");

                        match arg {
                            "b" | "breakpoints" => {
                                if session.breakpoint_index_tracker.is_empty() {
                                    println!("No breakpoint found");
                                    continue;
                                }
                                for (idx, bp) in session.breakpoint_index_tracker.iter().enumerate()
                                {
                                    // NOTE: User index is 1 based
                                    if let Some(bp) = bp {
                                        let user_idx = idx + 1;
                                        println!("{} {}", user_idx, bp);
                                    }
                                }
                            }
                            "r" | "reg" => {
                                if let Ok(regs) = session.get_regs() {
                                    println!("{}", regs);
                                } else {
                                    eprintln!("Failed to fetch registers");
                                }
                            }
                            _ => {
                                println!("Not handled yet")
                            }
                        }
                    }
                    "p" | "print" => {
                        let arg = parse_arg!(parts, "print");

                        let scope_result = session.find_current_scope();

                        match scope_result {
                            Ok(scope) => {
                                let var_result = session.get_var_value(&scope, arg);

                                match var_result {
                                    Ok(var) => {
                                        if let Some(val) = var {
                                            println!("{} = {}", arg, val);
                                        } else {
                                            eprintln!(
                                                "Var '{}' not in scope at current breakpoint",
                                                arg
                                            );
                                        }
                                    }
                                    Err(err) => eprintln!("Error occured parsing variable: {err}"),
                                }
                            }
                            Err(err) => eprintln!("Could not resolve current scope: {err}"),
                        }
                    }
                    "bt" | "backtrace" => {
                        let function_names = session.backtrace();
                        function_names.iter().for_each(|n| {
                            println!("{n}");
                        });
                    }
                    "q" | "quit" => {
                        // End current debug session
                        if let Err(err) = session.kill_session() {
                            eprintln!("{err}")
                        }
                        return Ok(());
                    }
                    "func" => {
                        let name = session
                            .metadata
                            .get_function_name(session.current_pc().unwrap());
                        println!("{:?}", name);
                    }
                    _ => {
                        println!("Not handled yet");
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("Use 'q' to quit current session")
            }
            Err(e) => eprintln!("{e}"),
        }
    }
}
