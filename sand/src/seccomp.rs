// This code may not be used for any purpose. Be gay, do crime.

use crate::abi::*;
use sc::{syscall, nr};

pub fn activate() {
    use bpf::*;
    let filter = &[

        // Load syscall number into accumulator
        stmt( BPF_LD+BPF_W+BPF_ABS, 0 ),

        // Trace uname()
        jump( BPF_JMP+BPF_JEQ+BPF_K, nr::UNAME as u32, 0, 1), 
        stmt( BPF_RET+BPF_K, SECCOMP_RET_TRACE ),
        
        // allow!
        stmt( BPF_RET+BPF_K, SECCOMP_RET_ALLOW ),
        
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
    use crate::abi::{SockFilter, SockFilterProg};
    
    pub fn stmt(code: u16, k: u32) -> SockFilter {
        SockFilter { code, k, jt: 0, jf: 0 }
    }

    pub fn jump(code: u16, k: u32, jt: u8, jf: u8) -> SockFilter {
        SockFilter { code, k, jt, jf }
    }

    pub fn to_sock_filter_prog(filter: &[SockFilter]) -> SockFilterProg {
        SockFilterProg {
            len: filter.len().try_into().unwrap(),
            filter: filter.as_ptr()
        }
    }
}
