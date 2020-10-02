// This code may not be used for any purpose. Be gay, do crime.

use crate::abi::*;
use sc::{syscall, nr};

pub fn activate() {
    use bpf::*;
    let filter = &[

        load_u32_absolute(offset_of!(SeccompData, nr) as u32),

        skip_unless_eq(nr::BRK as u32), ret(SECCOMP_RET_ALLOW),

        skip_unless_eq(nr::EXECVE as u32), ret(SECCOMP_RET_TRACE),
        skip_unless_eq(nr::CLOSE as u32), ret(SECCOMP_RET_TRACE),
        skip_unless_eq(nr::OPEN as u32), ret(SECCOMP_RET_TRACE),
        skip_unless_eq(nr::OPENAT as u32), ret(SECCOMP_RET_TRACE),
        //skip_unless_eq(nr::READ as u32), ret(SECCOMP_RET_TRACE),
        //skip_unless_eq(nr::WRITE as u32), ret(SECCOMP_RET_TRACE),
        skip_unless_eq(nr::UNAME as u32), ret(SECCOMP_RET_TRACE),
        
        // allow!
        ret(SECCOMP_RET_ALLOW),

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
