// This code may not be used for any purpose. Be gay, do crime.

// open
// linux/include/uapi/asm-generic/fcntl.h
pub const O_RDONLY: usize = 0;

// fcntl
// linux/include/uapi/linux/fcntl.h
pub const F_GET_SEALS: usize = 1034;

// F_GET_SEALS
// linux/include/uapi/linux/fcntl.h
pub const F_SEAL_SEAL: usize = 1;
pub const F_SEAL_SHRINK: usize = 2;
pub const F_SEAL_GROW: usize = 4;
pub const F_SEAL_WRITE: usize = 8;

// ptrace
// linux/include/uapi/linux/ptrace.h
pub const PTRACE_TRACEME: usize = 0;
pub const PTRACE_CONT: usize = 7;
pub const PTRACE_SETOPTIONS: usize = 0x4200;

// waitid
// linux/include/uapi/linux/wait.h
pub const P_ALL: usize = 0;
pub const WSTOPPED: usize = 2;
pub const WEXITED: usize = 4;
pub const WCONTINUED: usize = 8;
pub const SI_MAX_SIZE: usize = 128;

// errno
// linux/include/uapi/asm-generic/errno-base.h
pub const ECHILD: isize = -10;

// signo
// linux/include/uapi/asm-generic/signal.h
pub const SIGTRAP: u32 = 5;
pub const SIGCHLD: u32 = 17;
pub const SIGSTOP: u32 = 19;

// siginfo_t
// linux/include/uapi/asm-generic/siginfo.h
#[derive(Default, Debug)]
#[repr(C)]
pub struct SigInfo {
    pub si_signo: u32,
    pub si_errno: u32,
    pub si_code: u32,
    __pad0: u32,
    pub si_pid: u32,
    pub si_uid: u32,
    pub si_status: u32,
    __pad1: u32,
    pub si_utime: usize,
    pub si_stime: usize,
    pub fields: [u32; 20]
}

// si_code
// linux/include/uapi/asm-generic/siginfo.h
pub const CLD_EXITED: u32 = 1;
pub const CLD_KILLED: u32 = 2;
pub const CLD_DUMPED: u32 = 3;
pub const CLD_TRAPPED: u32 = 4;
pub const CLD_STOPPED: u32 = 5;
pub const CLD_CONTINUED: u32 = 6;
