#![allow(dead_code)]

// linux/include/uapi/asm-generic/fcntl.h
pub const O_ACCMODE: usize = 3;
pub const O_RDONLY: usize = 0;
pub const O_WRONLY: usize = 1;
pub const O_RDWR: usize = 2;
pub const F_SETFD: usize = 2;
pub const F_SETFL: usize = 4;
pub const F_SETOWN: usize = 8;
pub const F_CLOEXEC: usize = 1;
pub const FASYNC: usize = 0o20000;
pub const O_NONBLOCK: usize = 0o4000;
pub const O_DIRECTORY: usize = 0o200000;
pub const O_CLOEXEC: usize = 0o2000000;
pub const AT_SYMLINK_NOFOLLOW: i32 = 0x100;
pub const AT_FDCWD: i32 = -100;
pub const F_GET_SEALS: usize = 1034;
pub const F_SEAL_SEAL: usize = 1;
pub const F_SEAL_SHRINK: usize = 2;
pub const F_SEAL_GROW: usize = 4;
pub const F_SEAL_WRITE: usize = 8;

// getdents(2)
#[derive(Debug)]
#[repr(C)]
pub struct LinuxDirentHeader {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: u8,
}

// POSIX dirent.h or linux fs_types.h
pub const DT_UNKNOWN: u8 = 0;
pub const DT_FIFO: u8 = 1;
pub const DT_CHR: u8 = 2;
pub const DT_DIR: u8 = 4;
pub const DT_BLK: u8 = 6;
pub const DT_REG: u8 = 8;
pub const DT_LNK: u8 = 10;
pub const DT_SOCK: u8 = 12;
pub const DT_WHT: u8 = 14;

// ptrace
// linux/include/uapi/linux/ptrace.h
pub const PTRACE_TRACEME: usize = 0;
pub const PTRACE_POKEDATA: usize = 5;
pub const PTRACE_CONT: usize = 7;
pub const PTRACE_SINGLESTEP: usize = 9;
pub const PTRACE_SYSCALL: usize = 24;
pub const PTRACE_SETOPTIONS: usize = 0x4200;
pub const PTRACE_GETEVENTMSG: usize = 0x4201;
pub const PTRACE_GETREGSET: usize = 0x4204;
pub const PTRACE_SETREGSET: usize = 0x4205;
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
pub const PTRACE_SIG_TRACESYSGOOD: u32 = SIGTRAP | 0x80;
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

// linux/include/uapi/linux/mman.h
pub const MAP_PRIVATE: isize = 0x02;
pub const MAP_ANONYMOUS: isize = 0x20;
pub const MAP_FIXED: isize = 0x10;
pub const MAP_GROWSDOWN: isize = 0x100;
pub const MAP_FIXED_NOREPLACE: isize = 0x100000;

// linux/include/uapi/asm-generic/mman-common.h
pub const PROT_READ: isize = 1;
pub const PROT_WRITE: isize = 2;
pub const PROT_EXEC: isize = 4;

// ELF constant, used as ptrace user reg set identifier
pub const NT_PRSTATUS: usize = 1;

// iovec
// linux/include/uapi/linux/uio.h
#[derive(Debug, Clone)]
#[repr(C)]
pub struct IOVec {
    pub base: *mut u8,
    pub len: usize,
}

// user_regs_struct
// linux/arch/x86/include/asm/user_64.h
// linux/include/asm/user_64.h
#[derive(Default, Clone)]
#[repr(C)]
pub struct UserRegs {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub bp: u64,
    pub bx: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub ax: u64,
    pub cx: u64,
    pub dx: u64,
    pub si: u64,
    pub di: u64,
    pub orig_ax: u64,
    pub ip: u64,
    pub cs: u64,
    pub flags: u64,
    pub sp: u64,
    pub ss: u64,
    pub fs_base: u64,
    pub gs_base: u64,
    pub ds: u64,
    pub es: u64,
    pub fs: u64,
    pub gs: u64,
}

impl core::fmt::Debug for UserRegs {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            concat!(
                "UserRegs {{\n",
                "  cs={:16x}  ip={:16x}  ss={:16x}  sp={:16x}  bp={:16x} oax={:16x}\n",
                "  ax={:16x}  di={:16x}  si={:16x}  dx={:16x} r10={:16x}  r8={:16x}  r9={:16x}\n",
                "  bx={:16x}  cx={:16x} r11={:16x} r12={:16x} r13={:16x} r14={:16x} r15={:16x}\n",
                "  ds={:16x}  es={:16x}  fs={:16x}  gs={:16x} fs@={:16x} gs@={:16x} flg={:16x}\n",
                "}}"
            ),
            self.cs,
            self.ip,
            self.ss,
            self.sp,
            self.bp,
            self.orig_ax,
            self.ax,
            self.di,
            self.si,
            self.dx,
            self.r10,
            self.r8,
            self.r9,
            self.bx,
            self.cx,
            self.r11,
            self.r12,
            self.r13,
            self.r14,
            self.r15,
            self.ds,
            self.es,
            self.fs,
            self.gs,
            self.fs_base,
            self.gs_base,
            self.flags,
        )
    }
}

// ELF machine constants
// linux/include/uapi/linux/elf-em.h
pub const EM_386: u32 = 3;
pub const EM_X86_64: u32 = 62;

// linux/include/uapi/linux/auxvec.h
pub const AT_NULL: usize = 0;
pub const AT_IGNORE: usize = 1;
pub const AT_EXECFD: usize = 2;
pub const AT_PHDR: usize = 3;
pub const AT_PHENT: usize = 4;
pub const AT_PHNUM: usize = 5;
pub const AT_PAGESZ: usize = 6;
pub const AT_BASE: usize = 7;
pub const AT_FLAGS: usize = 8;
pub const AT_ENTRY: usize = 9;
pub const AT_NOTELF: usize = 10;
pub const AT_UID: usize = 11;
pub const AT_EUID: usize = 12;
pub const AT_GID: usize = 13;
pub const AT_EGID: usize = 14;
pub const AT_PLATFORM: usize = 15;
pub const AT_HWCAP: usize = 16;
pub const AT_CLKTCK: usize = 17;
pub const AT_SECURE: usize = 23;
pub const AT_BASE_PLATFORM: usize = 24;
pub const AT_RANDOM: usize = 25;
pub const AT_HWCAP2: usize = 26;
pub const AT_EXECFN: usize = 31;
pub const AT_SYSINFO: usize = 32;
pub const AT_SYSINFO_EHDR: usize = 33;

// linux/include/asm-generic/param.h
pub const USER_HZ: usize = 100;

// audit-architecture flags
// linux/include/uapi/linux/audit.h
pub const AUDIT_ARCH_LE: u32 = 0x40000000;
pub const AUDIT_ARCH_64BIT: u32 = 0x80000000;
pub const AUDIT_ARCH_I386: u32 = EM_386 | AUDIT_ARCH_LE;
pub const AUDIT_ARCH_X86_64: u32 = EM_X86_64 | AUDIT_ARCH_LE | AUDIT_ARCH_64BIT;

// ptrace_syscall_info
// linux/include/uapi/linux/ptrace.h
#[derive(Debug, Default)]
#[repr(C)]
pub struct SyscallInfo {
    pub op: u8,
    pub pad0: u8,
    pub pad1: u16,
    pub arch: u32,
    pub instruction_pointer: u64,
    pub stack_pointer: u64,
    pub nr: u64,
    pub args: [u64; 6],
    pub ret_data: u32,
}

impl SyscallInfo {
    pub fn from_regs(regs: &UserRegs) -> Self {
        SyscallInfo {
            op: PTRACE_SYSCALL_INFO_SECCOMP,
            arch: AUDIT_ARCH_X86_64,
            instruction_pointer: regs.ip,
            stack_pointer: regs.sp,
            nr: regs.orig_ax,
            args: [regs.di, regs.si, regs.dx, regs.r10, regs.r8, regs.r9],
            ..Default::default()
        }
    }

    pub fn args_to_regs(args: &[isize], regs: &mut UserRegs) {
        assert!(args.len() <= 6);
        regs.di = *args.get(0).unwrap_or(&0) as u64;
        regs.si = *args.get(1).unwrap_or(&0) as u64;
        regs.dx = *args.get(2).unwrap_or(&0) as u64;
        regs.r10 = *args.get(3).unwrap_or(&0) as u64;
        regs.r8 = *args.get(4).unwrap_or(&0) as u64;
        regs.r9 = *args.get(5).unwrap_or(&0) as u64;
    }

    pub fn nr_to_regs(nr: isize, regs: &mut UserRegs) {
        regs.ax = nr as u64;
    }

    pub fn ret_to_regs(ret_data: isize, regs: &mut UserRegs) {
        regs.ax = ret_data as u64;
    }

    pub fn ret_from_regs(regs: &UserRegs) -> isize {
        regs.ax as isize
    }

    // syscall number to resume, or SYSCALL_BLOCKED to skip
    pub fn orig_nr_to_regs(nr: isize, regs: &mut UserRegs) {
        regs.orig_ax = nr as u64;
    }
}

// Special syscall number
pub const SYSCALL_BLOCKED: isize = -1;

// waitid
// linux/include/uapi/linux/wait.h
pub const P_ALL: usize = 0;
pub const WSTOPPED: usize = 2;
pub const WEXITED: usize = 4;
pub const WCONTINUED: usize = 8;
pub const SI_MAX_SIZE: usize = 128;

// errno
// linux/include/uapi/asm-generic/errno-base.h
pub const EINTR: i32 = 4;
pub const EIO: i32 = 5;
pub const E2BIG: i32 = 7;
pub const ENOEXEC: i32 = 8;
pub const ECHILD: i32 = 10;
pub const EAGAIN: i32 = 11;
pub const EFAULT: i32 = 14;
pub const EEXIST: i32 = 17;
pub const EINVAL: i32 = 22;
pub const ENOSYS: i32 = 38;
pub const ECONNRESET: i32 = 104;

// signo
// linux/include/uapi/asm-generic/signal.h
pub const SIGINT: u32 = 2;
pub const SIGTRAP: u32 = 5;
pub const SIGBUS: u32 = 7;
pub const SIGKILL: u32 = 9;
pub const SIGUSR1: u32 = 10;
pub const SIGUSR2: u32 = 12;
pub const SIGSEGV: u32 = 11;
pub const SIGCHLD: u32 = 17;
pub const SIGCONT: u32 = 18;
pub const SIGSTOP: u32 = 19;
pub const SIGURG: u32 = 23;
pub const SIGIO: u32 = 29;
pub const SIGSYS: u32 = 31;

// linux/include/uapi/linux/fs.h
pub const SEEK_SET: isize = 0;
pub const SEEK_CUR: isize = 1;
pub const SEEK_END: isize = 2;
pub const SEEK_DATA: isize = 3;
pub const SEEK_HOLE: isize = 4;
pub const SEEK_MAX: isize = SEEK_HOLE;

// sendmsg() user_msghdr
// linux/include/linux/socket.h
#[derive(Debug, Clone)]
#[repr(C)]
pub struct MsgHdr {
    pub msg_name: *mut usize,
    pub msg_namelen: i32,
    pub msg_iov: *mut IOVec,
    pub msg_iovlen: usize,
    pub msg_control: *mut usize,
    pub msg_controllen: usize,
    pub msg_flags: u32,
}

// sendmsg()
// linux/include/linux/socket.h
pub const MSG_DONTWAIT: usize = 0x40;

// linux/include/linux/socket.h
#[derive(Default, Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct CMsgHdr {
    pub cmsg_len: usize,
    pub cmsg_level: i32,
    pub cmsg_type: i32,
}

// cmsg_type
// linux/include/linux/socket.h
pub const SCM_RIGHTS: i32 = 1;

// cmsg_level
// linux/include/uapi/asm-generic/socket.h
pub const SOL_SOCKET: i32 = 1;

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
    pub fields: [u32; 20],
}

// si_code
// linux/include/uapi/asm-generic/siginfo.h
pub const CLD_EXITED: u32 = 1;
pub const CLD_KILLED: u32 = 2;
pub const CLD_DUMPED: u32 = 3;
pub const CLD_TRAPPED: u32 = 4;
pub const CLD_STOPPED: u32 = 5;
pub const CLD_CONTINUED: u32 = 6;

// sigaction
// linux/inclide/linux/signal_types.h
#[derive(Debug)]
#[repr(C)]
pub struct SigAction {
    pub sa_handler: extern "C" fn(u32),
    pub sa_flags: u32,
    pub sa_restorer: unsafe extern "C" fn(),
    pub sa_mask: [u64; 16],
}

// arch/x86/include/uapi/asm/signal.h
pub const SA_RESTORER: u32 = 0x04000000;

/// sigset_t
/// linux/include/uapi/asm-generic/signal.h
#[derive(Debug)]
#[repr(C)]
pub struct SigSet {
    pub sig: [u64; 1],
}

/// linux/include/uapi/linux/binfmts.h
pub const BINPRM_BUF_SIZE: usize = 256;

/// linux/include/linux/socket.h
pub const AF_UNIX: usize = 1;

/// linux/include/linux/net.h
pub const SOCK_STREAM: usize = 1;

/// linux/arch/x86/include/asm/page_types.h
pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
pub const PAGE_MASK: usize = PAGE_SIZE - 1;

pub fn page_offset(addr: usize) -> usize {
    addr & PAGE_MASK
}

pub fn page_round_down(addr: usize) -> usize {
    addr & !PAGE_MASK
}

pub fn page_round_up(addr: usize) -> usize {
    if page_offset(addr) == 0 {
        addr
    } else {
        PAGE_SIZE + page_round_down(addr)
    }
}

/// linux/include/uapi/linux/time.h
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TimeSpec {
    pub tv_sec: u64,
    pub tv_nsec: u64,
}

impl TimeSpec {
    #[allow(dead_code)]
    pub fn from_secs(n: u64) -> Self {
        TimeSpec {
            tv_sec: n,
            tv_nsec: 0,
        }
    }
}

/// linux/arch/x86/include/asm/elf.h (64-bit)
pub const STACK_RND_MASK: usize = 0x3fffff;

/// by analogy, the brk randomization mask, linux has this hardcoded in
/// linux/arch/x86/kernel/process.c
pub const BRK_RND_MASK: usize = 0x1fff;

/// linux/include/uapi/linux/utsname.h
#[derive(Debug, Clone)]
#[repr(C)]
pub struct UtsName {
    pub sysname: [u8; 65],
    pub nodename: [u8; 65],
    pub release: [u8; 65],
    pub version: [u8; 65],
    pub machine: [u8; 65],
}

pub const PLATFORM_NAME_BYTES: &[u8] = b"x86_64\0";
