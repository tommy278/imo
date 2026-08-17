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
