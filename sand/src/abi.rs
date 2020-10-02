// This code may not be used for any purpose. Be gay, do crime.

// open
pub const O_RDONLY: usize = 0;

// fcntl
pub const F_GET_SEALS: usize = 1034;

// F_GET_SEALS
pub const F_SEAL_SEAL: usize = 1;
pub const F_SEAL_SHRINK: usize = 2;
pub const F_SEAL_GROW: usize = 4;
pub const F_SEAL_WRITE: usize = 8;

// waitid
pub const P_ALL: usize = 0;

// siginfo_t
#[derive(Default, Debug)]
#[repr(C)]
pub struct SigInfo {
    pub si_signo: u32,
    pub si_errno: u32,
    pub si_code: u32,
    __pad0: u32,
    pub fields: [u32; 28]
}

