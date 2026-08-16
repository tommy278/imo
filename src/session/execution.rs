use crate::types::StringId;

// The command to be ran when the debugger hits a sigtrap
#[derive(Default, Debug)]
pub enum CurrentStopCmd {
    SingleStep,
    StepOver {
        start_stack_pointer: u64,
        start_file: StringId,
        start_line: u32,
        started_from_inline: bool,
    },
    StepInto {
        start_file: StringId,
        start_line: u32,
    },
    StepOut {
        original_stack_pointer: u64,
        original_file: StringId,
        original_line: u32,
        return_address: u64,
        started_from_inline: bool,
    },
    FinishStepOver {
        original_stack_pointer: u64,
        original_file: StringId,
        original_line: u32,
        started_from_inline: bool,
    },
    Finish {
        start_stack_pointer: u64,
        started_from_inline: bool,
    },
    StepOutFinish {
        original_stack_pointer: u64,
        return_address: u64,
        started_from_inline: bool,
    },
    CompleteFinish {
        start_stack_pointer: u64,
        started_from_inline: bool,
    },
    #[default]
    Idle,
    Running,
    Continuing,
    SearchingForValidLocation,
    Completed,
}

impl CurrentStopCmd {
    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed)
    }

    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }
}

// If not supported yet, add dummy values for compilation
pub mod default_os {
    // Dummy types to satisfy the type aliases
    pub type ProcessId = i32;
    pub type PlatformRegStruct = ();

    use crate::{helpers::dwarf::error::CacheSetupError, session::error::SystemError};

    #[derive(Debug, Clone)]
    pub struct PlatformBreakpoint;

    impl PlatformBreakpoint {
        pub fn new(_absolute_address: u64) -> Self {
            unimplemented!("imo debugger only runs on linux")
        }
        pub fn enable(&self, _pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on linux")
        }
        pub fn disable(&self, _pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on linux")
        }
    }

    pub fn send_trap_signal(_pid: ProcessId) -> Result<(), SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn get_process_base_address(_pid: ProcessId) -> Result<u64, CacheSetupError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn read_bytes(_pid: ProcessId, _ptr: usize, _len: usize) -> Result<Vec<u8>, SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn step(_pid: ProcessId) -> Result<(), SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn continue_session(_pid: ProcessId) -> Result<(), SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn kill_session(_pid: ProcessId) -> Result<(), SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn peek_data(_pid: ProcessId, _address: u64) -> Result<i64, SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }

    pub fn get_regs(_pid: ProcessId) -> Result<PlatformRegStruct, SystemError> {
        unimplemented!("imo debugger only runs on Linux")
    }
}
