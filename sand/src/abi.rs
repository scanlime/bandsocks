// This code may not be used for any purpose. Be gay, do crime.

#![allow(dead_code)]

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
pub const PTRACE_GETEVENTMSG: usize = 0x4201;
pub const PTRACE_GET_SYSCALL_INFO: usize = 0x420e;
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
pub const PTRACE_SIG_FORK: u32 = SIGTRAP | (PTRACE_EVENT_FORK << 8) as u32;
pub const PTRACE_SIG_VFORK: u32 = SIGTRAP | (PTRACE_EVENT_VFORK << 8) as u32;
pub const PTRACE_SIG_CLONE: u32 = SIGTRAP | (PTRACE_EVENT_CLONE << 8) as u32;
pub const PTRACE_SIG_EXEC: u32 = SIGTRAP | (PTRACE_EVENT_EXEC << 8) as u32;
pub const PTRACE_SIG_VFORK_DONE: u32 = SIGTRAP | (PTRACE_EVENT_VFORK_DONE << 8) as u32;
pub const PTRACE_SIG_SECCOMP: u32 = SIGTRAP | (PTRACE_EVENT_SECCOMP << 8) as u32;
pub const PTRACE_SYSCALL_INFO_NONE: u8 = 0;
pub const PTRACE_SYSCALL_INFO_ENTRY: u8 = 1;
pub const PTRACE_SYSCALL_INFO_EXIT: u8 = 2;
pub const PTRACE_SYSCALL_INFO_SECCOMP: u8 = 3;

// ptrace_syscall_info
// linux/include/uapi/linux/ptrace.h
#[derive(Default)]
#[repr(C, packed)]
pub struct PTraceSyscallInfo {
    pub op: u8,
    pad0: u8,
    pad1: u16,
    pub arch: u32,
    pub instruction_pointer: u64,
    pub stack_pointer: u64,
    pub nr: u64,
    pub args: [u64; 6],
    pub ret_data: u32,
}

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

// sock_fprog
// seccomp(2)
#[derive(Debug)]
#[repr(C)]
pub struct SockFilterProg {
    pub len: u16,
    pub filter: *const SockFilter,
}

// sock_filter
// seccomp(2)
// linux/include/uapi/linux/filter.hOB
#[derive(Debug)]
#[repr(C)]
pub struct SockFilter {
    pub code: u16,
    pub jt: u8,
    pub jf: u8,
    pub k: u32,
}

// seccomp_data
// seccomp(2)
#[derive(Debug)]
#[repr(C)]
pub struct SeccompData {
    pub nr: i32,
    pub arch: u32,
    pub instruction_pointer: u64,
    pub args: [u64; 6]
}

// seccomp filter return values
// linux/include/uapi/linux/seccomp.h
pub const SECCOMP_RET_KILL_PROCESS: u32 = 0x80000000;
pub const SECCOMP_RET_KILL_THREAD: u32 = 0x00000000;
pub const SECCOMP_RET_TRAP: u32 = 0x00030000;
pub const SECCOMP_RET_ERRNO: u32 = 0x00050000;
pub const SECCOMP_RET_TRACE: u32 = 0x7ff00000;
pub const SECCOMP_RET_LOG: u32 = 0x7ffc0000;
pub const SECCOMP_RET_ALLOW: u32 = 0x7fff0000;

// bpf instruction classes
// linux/include/uapi/linux/bpf_common.h
pub const BPF_LD: u16 = 0x00;
pub const BPF_LDX: u16 = 0x01;
pub const BPF_ST: u16 = 0x02;
pub const BPF_STX: u16 = 0x03;
pub const BPF_ALU: u16 = 0x04;
pub const BPF_JMP: u16 = 0x05;
pub const BPF_RET: u16 = 0x06;
pub const BPF_MISX: u16 = 0x07;

// bpf data width
pub const BPF_W: u16 = 0x00;
pub const BPF_H: u16 = 0x08;
pub const BPF_B: u16 = 0x10;
pub const BPF_DW: u16 = 0x18;

// bpf data modes
pub const BPF_IMM: u16 = 0x00;
pub const BPF_ABS: u16 = 0x20;
pub const BPF_IND: u16 = 0x40;
pub const BPF_MEM: u16 = 0x60;
pub const BPF_LEN: u16 = 0x80;
pub const BPF_MSH: u16 = 0xa0;

// bpf source field
pub const BPF_K: u16 = 0x00;
pub const BPF_X: u16 = 0x08;

// bpf jump codes
pub const BPF_JA: u16 = 0x00;
pub const BPF_JEQ: u16 = 0x10;
pub const BPF_JGT: u16 = 0x20;
pub const BPF_JGE: u16 = 0x30;
pub const BPF_JSET: u16 = 0x40;

// bpf alu operations
pub const BPF_ADD: u16 = 0x00;
pub const BPF_SUB: u16 = 0x10;
pub const BPF_MUL: u16 = 0x20;
pub const BPF_DIV: u16 = 0x30;
pub const BPF_OR: u16 = 0x40;
pub const BPF_AND: u16 = 0x50;
pub const BPF_LSH: u16 = 0x60;
pub const BPF_RSH: u16 = 0x70;
pub const BPF_NEG: u16 = 0x80;
pub const BPF_MOD: u16 = 0x90;
pub const BPF_XOR: u16 = 0xa0;
