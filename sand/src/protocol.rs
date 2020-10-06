// The protocol is defined here canonically and then imported
// by the runtime crate along with our finished binary.

pub use ssmarshal::{serialize, deserialize};

pub const BUFFER_SIZE: usize = 128;

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct SysPid(pub u32);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[repr(C)]
pub enum MessageToSand {
    Nop
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[repr(C)]
pub enum MessageFromSand {
    Nop(usize, usize)
}
