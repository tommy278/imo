use crate::utils::trim_file_path;

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

                if let Some((stripped_name, _)) = func_name.split_once("<") {
                    write!(
                        f,
                        "{:<36} {:#x} @ {}:{}",
                        stripped_name, rip, trimmed_file, line
                    )?;
                } else {
                    let formatted_func_name = format!("{}()", func_name);
                    write!(
                        f,
                        "{:<36} {:#x} @ {}:{}",
                        formatted_func_name, rip, trimmed_file, line
                    )?;
                }
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

                if let Some((stripped_name, _)) = func_name.split_once("<") {
                    write!(
                        f,
                        "{:<36} {:#x} [INLINED]\n\tDeclared @ {}:{} \n\tCalled @ {}:{} ",
                        stripped_name,
                        rip,
                        trimmed_decl_file,
                        decl_line,
                        trimmed_call_file,
                        call_line
                    )?;
                } else {
                    write!(
                        f,
                        "{:<36} {:#x} [INLINED]\n\tDeclared @ {}:{} \n\tCalled @ {}:{} ",
                        func_name, rip, trimmed_decl_file, decl_line, trimmed_call_file, call_line
                    )?;
                }
            }
        }

        Ok(())
    }
}
