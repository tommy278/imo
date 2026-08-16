use crate::session::os;

#[derive(Debug)]
pub struct ManagedBreakpoint {
    pub breakpoint: os::PlatformBreakpoint,
    pub ref_count: usize,
}

impl ManagedBreakpoint {
    pub fn new(breakpoint: os::PlatformBreakpoint) -> Self {
        Self {
            breakpoint,
            ref_count: 1,
        }
    }
}
