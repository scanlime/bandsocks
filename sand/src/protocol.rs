// This code may not be used for any purpose. Be gay, do crime.

// The protocol is defined here canonically and then imported
// by the runtime crate along with our finished binary.

use ssmarshal::{serialize, deserialize};

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct SysPid(pub u32);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[repr(C)]
enum MessageToSand {
    Nop
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[repr(C)]
enum MessageFromSand {
    Nop
}
