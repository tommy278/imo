use nix::sys::ptrace;
use nix::unistd::Pid;

/// Stores and manages the insertion and removal of the 0xCC (INT3) keyword for the instruction
pub struct BreakPoint {
    addr: u64,
    original_byte: u8,
    is_enabled: bool,
}

impl BreakPoint {
    /// Creates a new instance of breakpoint from address
    pub fn new(addr: u64) -> Self {
        Self {
            addr,
            original_byte: 0,
            is_enabled: false,
        }
    }

    /// Overwrite the lowest byte with 0xCC (INT3) while saving the original byte
    pub fn enable(&mut self, pid: Pid) {
        // Read the current memory word
        let word = ptrace::read(pid, self.addr as ptrace::AddressType).unwrap();

        // Save the original lowest byte
        self.original_byte = (word & 0xFF) as u8;

        // Overwrite the lowest byte with 0xCC (INT3)
        let breakpoint_word = (word & !0xFF) | 0xCC;

        // Write word back to child memory
        unsafe {
            ptrace::write(pid, self.addr as ptrace::AddressType, breakpoint_word).unwrap();
        }
        self.is_enabled = true;
    }

    /// Swaps 0xCC out, puts original byte back
    pub fn disable(&mut self, pid: Pid) {
        if !self.is_enabled {
            return;
        }

        let word = ptrace::read(pid, self.addr as ptrace::AddressType).unwrap();
        let restored_word = (word & !0xFF) | (self.original_byte as i64);

        unsafe {
            ptrace::write(pid, self.addr as ptrace::AddressType, restored_word);
        }
        self.is_enabled = false;
    }
}
