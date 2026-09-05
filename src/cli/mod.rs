pub mod utils;

use rustyline::error::ReadlineError;
use rustyline::{DefaultEditor, Result};

use crate::dwarf::debug_info::ParamType;
use crate::session::DebugSession;
use owo_colors::OwoColorize;
use utils::{
    display_backtrace_help, display_break_help, display_clear_help, display_continue_help,
    display_delete_help, display_disable_help, display_enable_help, display_error,
    display_finish_help, display_help, display_info_help, display_list_help, display_next_help,
    display_print_help, display_quit_help, display_single_step_help, display_step_help,
    flush_output, get_breakpoint_address, handle_breakpoint_clearing, handle_breakpoint_setting,
    handle_breakpoint_with_addr, handle_cmd, handle_event_by_index, parse_arg, parse_line_arg,
};

/// Display an interactive menu at breakpoints
pub fn handle_user_debugger_menu(session: &mut DebugSession, rl: &mut DefaultEditor) -> Result<()> {
    loop {
        let read_line = rl.readline("(\x1b[36mimo\x1b[0m) ");

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
                                eprintln!("{err}")
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
                                eprintln!("{err}")
                            }
                            return Ok(());
                        } else {
                            println!("Session not started yet")
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

                        // Breaking at raw addresses
                        if arg.starts_with('*') {
                            let breakpoint_addr = get_breakpoint_address!(arg);
                            handle_breakpoint_with_addr!(create, session, breakpoint_addr);
                            continue;
                        }

                        // Handle setting breakpoint if filename and line_number are provided
                        // eg: break running_task:6
                        if let Some((file_name, line_num)) = arg.split_once(":") {
                            let line_number = parse_line_arg!(line_num, u32);

                            let line_index =
                                session.get_specific_breakpoint_target(file_name, line_number);
                            handle_breakpoint_setting(session, &line_index, line_number);
                            continue;
                        }

                        if let Some(breakpoint_addr) = session.get_func_low_pc(arg) {
                            handle_breakpoint_with_addr!(create, session, breakpoint_addr);
                            continue;
                        }

                        // Handle setting breakpoint if only line_number is provided
                        // eg: break 12

                        let line_number = parse_line_arg!(arg, u32);
                        let Some(line_index) = session.get_breakpoint_target(line_number) else {
                            display_error!("Cannot set breakpoint at target");
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

                        if let Some(breakpoint_addr) = session.get_func_low_pc(arg) {
                            handle_breakpoint_with_addr!(clear, session, breakpoint_addr);
                            continue;
                        }

                        if arg.starts_with('*') {
                            let breakpoint_addr = get_breakpoint_address!(arg);
                            handle_breakpoint_with_addr!(clear, session, breakpoint_addr);
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
                            "b" | "break" | "breakpoints" => {
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
                            "r" | "reg" | "regs" | "register" | "registers" => {
                                if let Ok(regs) = session.get_regs() {
                                    println!("{}", regs);
                                } else {
                                    display_error!("Failed to fetch registers");
                                }
                            }
                            "lo" | "local" | "locals" => {
                                match session
                                    .get_all_values_for_specified_param(ParamType::Variable)
                                {
                                    Ok(field) => {
                                        if field.is_empty() {
                                            println!("No variable in scope");
                                        }

                                        for (name, val) in field.iter() {
                                            print!("{} = ", name);
                                            flush_output();

                                            if let Some(val) = val {
                                                println!("{}", val);
                                            } else {
                                                display_error!("<Could not parse variable>")
                                            }
                                        }
                                    }
                                    Err(e) => display_error!("{}", e),
                                }
                            }
                            "a" | "arg" | "args" | "argument" | "arguments" => {
                                match session
                                    .get_all_values_for_specified_param(ParamType::Argument)
                                {
                                    Ok(field) => {
                                        if field.is_empty() {
                                            println!("No argument in scope");
                                        }

                                        for (name, val) in field.iter() {
                                            print!("{} = ", name);
                                            flush_output();

                                            if let Some(val) = val {
                                                println!("{}", val);
                                            } else {
                                                display_error!("<Could not parse argument>")
                                            }
                                        }
                                    }
                                    Err(e) => display_error!("{}", e),
                                }
                            }
                            _ => {
                                display_error!(
                                    "Unsupported command: Use help for valid valid commands"
                                )
                            }
                        }
                    }
                    "p" | "print" => {
                        let arg = parse_arg!(parts, "print");

                        let scope_result = session.find_specified_param(ParamType::All);
                        match scope_result {
                            Ok(scope) => {
                                let param_result = session.get_param_value(&scope, arg);

                                match param_result {
                                    Ok(param) => {
                                        if let Some(val) = param {
                                            println!("{} = {}", arg, val);
                                        } else {
                                            display_error!(
                                                "Var '{}' not in scope at current breakpoint",
                                                arg
                                            );
                                        }
                                    }
                                    Err(err) => {
                                        display_error!("Error occured parsing variable: {}", err)
                                    }
                                }
                            }
                            Err(err) => display_error!("Could not resolve current scope: {}", err),
                        }
                    }
                    "bt" | "backtrace" => match session.backtrace() {
                        Ok(stack_frames) => {
                            if stack_frames.is_empty() {
                                display_error!(
                                    "Could not find stack frame info for current location"
                                );
                            }

                            let mut frame = 0;
                            stack_frames.iter().for_each(|f| {
                                println!("#{:<4} {}", frame, f);
                                frame += 1;
                            });
                        }
                        Err(e) => {
                            display_error!("{}", e)
                        }
                    },
                    "l" | "ls" | "list" => {
                        if !session.is_idle() {
                            match session.get_current_list_entry() {
                                Some(list) => {
                                    list.iter().for_each(|s| {
                                        println!("{s}");
                                    });
                                }
                                None => {
                                    display_error!("Could not resolve path");
                                }
                            }
                        } else {
                            println!("Session not started yet")
                        }
                    }
                    "q" | "quit" => {
                        // End current debug session
                        if let Err(e) = session.kill_session() {
                            display_error!("{}", e)
                        }
                        return Ok(());
                    }
                    "h" | "help" => {
                        if let Some(arg) = parts.next() {
                            match arg {
                                "b" | "break" => display_break_help(),
                                "clear" => display_clear_help(),
                                "e" | "enable" => display_enable_help(),
                                "dis" | "disable" => display_disable_help(),
                                "d" | "delete" => display_delete_help(),
                                "c" | "cont" | "continue" => display_continue_help(),
                                "n" | "next" => display_next_help(),
                                "si" | "stepi" => display_single_step_help(),
                                "s" | "step" => display_step_help(),
                                "f" | "fin" | "finish" => display_finish_help(),
                                "p" | "print" => display_print_help(),
                                "i" | "info" => display_info_help(),
                                "bt" | "backtrace" => display_backtrace_help(),
                                "l" | "ls" | "list" => display_list_help(),
                                "q" | "quit" => display_quit_help(),
                                _ => eprintln!("No help entry exists for {} cmd", arg.red()),
                            }
                            continue;
                        }

                        display_help()
                    }
                    _ => {
                        display_error!("Unsupported command. Use 'help' for valid commands");
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("Use 'q' to quit current session")
            }
            Err(e) => display_error!("{}", e),
        }
    }
}
