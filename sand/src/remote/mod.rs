pub mod mem;
pub mod scratchpad;
pub mod trampoline;

#[derive(Debug, Clone, Eq, PartialEq)]
#[repr(C)]
pub struct RemoteFd(pub u32);
