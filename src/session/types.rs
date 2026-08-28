use crate::utils::trim_file_path;

#[derive(Debug, Copy, Clone)]
pub struct StackInfo<'a> {
    pub func_name: &'a str,
    pub file: &'a str,
    pub rip: u64,
    pub line: u32,
}

impl std::fmt::Display for StackInfo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let trimmed_file = trim_file_path(&self.file);

        if let Some((stripped_name, _)) = self.func_name.split_once("<") {
            write!(
                f,
                "{:<36} {:#x} @ {}:{}",
                stripped_name, self.rip, trimmed_file, self.line
            )?;
        } else {
            let formatted_func_name = format!("{}()", self.func_name);
            write!(
                f,
                "{:<36} {:#x} @ {}:{}",
                formatted_func_name, self.rip, trimmed_file, self.line
            )?;
        }

        Ok(())
    }
}
