// This code may not be used for any purpose. Be gay, do crime.

use crate::abi::*;
use core::convert::TryInto;
use sc::syscall;

fn bpf_stmt(code: u16, k: u32) -> SockFilter {
    SockFilter { code, k, jt: 0, jf: 0 }
}

fn bpf_jump(code: u16, k: u32, jt: u8, jf: u8) -> SockFilter {
    SockFilter { code, k, jt, jf }
}

fn to_sock_filter_prog(filter: &[SockFilter]) -> SockFilterProg {
    SockFilterProg {
        len: filter.len().try_into().unwrap(),
        filter: filter.as_ptr()
    }
}

pub fn activate() {
    let filter = &[
    
        bpf_stmt( BPF_RET | BPF_K, SECCOMP_RET_KILL_PROCESS ),
        
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
