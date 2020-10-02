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
pub const PTRACE_EVENT_FORK: usize = 1;
pub const PTRACE_EVENT_VFORK: usize = 2;
pub const PTRACE_EVENT_CLONE: usize = 3;
pub const PTRACE_EVENT_EXEC: usize = 4;
pub const PTRACE_EVENT_VFORK_DONE: usize = 5;
pub const PTRACE_EVENT_SECCOMP: usize = 7;
pub const PTRACE_O_TRACESYSGOOD: usize = 1;
pub const PTRACE_O_TRACEFORK: usize = 1 << PTRACE_EVENT_FORK;
pub const PTRACE_O_TRACEVFORK: usize = 1 << PTRACE_EVENT_VFORK;
pub const PTRACE_O_TRACECLONE: usize = 1 << PTRACE_EVENT_CLONE;
pub const PTRACE_O_TRACEEXEC: usize = 1 << PTRACE_EVENT_EXEC;
pub const PTRACE_O_TRACEVFORK_DONE: usize = 1 << PTRACE_EVENT_VFORK_DONE;
pub const PTRACE_O_TRACESECCOMP: usize = 1 << PTRACE_EVENT_SECCOMP;
pub const PTRACE_O_EXITKILL: usize = 1 << 20;

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
    pad0: u32,
    pub si_pid: u32,
    pub si_uid: u32,
    pub si_status: u32,
    pad1: u32,
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

// prctl
// linux/include/uapi/linux/prctl.h
pub const PR_SET_NO_NEW_PRIVS: usize = 38;
pub const PR_SET_SECCOMP: usize = 22;
pub const SECCOMP_MODE_FILTER: usize = 2;
