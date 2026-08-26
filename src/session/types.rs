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

        write!(f, "{}", self.func_name)?;

        if !self.func_name.contains("<") {
            write!(f, "() ")?;
        } else {
            write!(f, " ")?;
        }

        write!(f, "{:#x} @ {}:{}", self.rip, trimmed_file, self.line)?;
        Ok(())
    }
}
