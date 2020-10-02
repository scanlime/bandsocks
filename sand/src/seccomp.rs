// This code may not be used for any purpose. Be gay, do crime.

use crate::abi::*;
use sc::{syscall, nr};

pub fn activate() {
    // Build and install a cBPF (classic Berkeley Packet Filter)
    // program, which runs at every tracee syscall, with the opportunity
    // to take an action and/or to modify the syscall.
    
    use bpf::*;
    let filter = &[

        // Examine syscall number
        load_u32_absolute(offset_of!(SeccompData, nr) as u32),

        // Basic syscalls from seccomp 'strict' mode
        skip_unless_eq(nr::READ as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::WRITE as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::EXIT as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::RT_SIGRETURN as u32), ret(SECCOMP_RET_ALLOW),
        
        // xxx everything below here is purely experimental

        // syscalls this binary currently needs
        // xxx: some of these do need sandboxing. either we have
        //      to remove functionality from this binary, or make
        //      sure tracing works in all of these cases, or do
        //      something else. (handle via signal or bpf)
        skip_unless_eq(nr::BRK as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::ARCH_PRCTL as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::PRCTL as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::ACCESS as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::OPEN as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::OPENAT as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::CLOSE as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::FCNTL as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::MMAP as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::MPROTECT as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::FORK as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::EXECVE as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::WAITID as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::PTRACE as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::GETPID as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::PTRACE as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::KILL as u32), ret(SECCOMP_RET_ALLOW),
        skip_unless_eq(nr::GETPID as u32), ret(SECCOMP_RET_ALLOW),
        
        // xxx temp: trace-all instead of deny-all for now
        ret(SECCOMP_RET_TRACE),
    ];
    let prog = to_sock_filter_prog(filter);
    let ptr = (&prog) as *const SockFilterProg as usize;
    let result = unsafe {
        syscall!(PRCTL, PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
        syscall!(PRCTL, PR_SET_SECCOMP, SECCOMP_MODE_FILTER, ptr, 0, 0) as isize
    };
    if result != 0 {
        panic!("seccomp setup error ({})", result);
    }
}

// https://man.openbsd.org/bpf.4
mod bpf {
    use core::convert::TryInto;
    use crate::abi::*;
    
    pub fn stmt(code: u16, k: u32) -> SockFilter {
        SockFilter { code, k, jt: 0, jf: 0 }
    }

    pub fn jump(code: u16, k: u32, jt: u8, jf: u8) -> SockFilter {
        SockFilter { code, k, jt, jf }
    }

    pub fn load_u32_absolute(k: u32) -> SockFilter {        
        stmt( BPF_LD+BPF_W+BPF_ABS, k )
    }

    pub fn ret(k: u32) -> SockFilter {
        stmt( BPF_RET+BPF_K, k )
    }

    pub fn skip_unless_eq(k: u32) -> SockFilter {
        jump( BPF_JMP+BPF_JEQ+BPF_K, k, 0, 1)
    }
    
    pub fn to_sock_filter_prog(filter: &[SockFilter]) -> SockFilterProg {
        SockFilterProg {
            len: filter.len().try_into().unwrap(),
            filter: filter.as_ptr()
        }
    }
}
