use crate::utils::trim_file_path;
use owo_colors::OwoColorize;

#[derive(Debug, Copy, Clone)]
pub enum StackInfo<'a> {
    Function {
        func_name: &'a str,
        file: &'a str,
        rip: u64,
        line: u32,
    },
    Inlined {
        func_name: &'a str,
        decl_file: &'a str,
        call_file: &'a str,
        rip: u64,
        call_line: u32,
        decl_line: u32,
    },
}

impl std::fmt::Display for StackInfo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Function {
                func_name,
                file,
                rip,
                line,
            } => {
                let trimmed_file = trim_file_path(file);

                let (cleaned_name, is_normal_func) = match func_name.split_once("<") {
                    Some((stripped_name, _)) => (stripped_name, false),
                    None => (*func_name, true),
                };

                if is_normal_func {
                    // Get the padding by figuring out our length after the parentheses
                    // This is the len of the string + 2 (size of parentheses)
                    // Subtract by 36 and fill the space up to match the other branch
                    let base_len = cleaned_name.len() + 2;
                    let padding = 36_usize.saturating_sub(base_len);
                    write!(
                        f,
                        "{}{}{:<width$}",
                        cleaned_name.blue(),
                        "()",
                        "",
                        width = padding
                    )?;
                } else {
                    write!(f, "{:<36}", cleaned_name.blue())?;
                };

                write!(f, "{:#x} @ {}:{}", rip, trimmed_file.green(), line)?;

                Ok(())
            }
            Self::Inlined {
                func_name,
                decl_file,
                call_file,
                rip,
                call_line,
                decl_line,
            } => {
                let trimmed_decl_file = trim_file_path(decl_file);
                let trimmed_call_file = trim_file_path(call_file);

                let cleaned_name = match func_name.split_once("<") {
                    Some((stripped_name, _)) => stripped_name,
                    None => *func_name,
                };

                write!(
                    f,
                    "{:<36}{:#x} [INLINED]\n\t{:<12} @ {}:{} \n\t{:<12} @ {}:{}",
                    cleaned_name.fg_rgb::<180, 180, 180>(),
                    rip,
                    "Declared",
                    trimmed_decl_file.green(),
                    decl_line,
                    "Called",
                    trimmed_call_file.green(),
                    call_line
                )?;

                Ok(())
            }
        }
    }
}
