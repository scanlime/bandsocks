// This code may not be used for any purpose. Be gay, do crime.

use crate::bpf::*;
use crate::abi::*;
use sc::{syscall, nr};

fn filter(p: &mut ProgramBuffer) {
    // Examine syscall number
    p.inst(load(offset_of!(SeccompData, nr)));

    // List of fully allowed calls
    // xxx: pare this down as much as possible
    // xxx: audit source code for anything we leave allowed
    p.if_any_eq(&[

        nr::READ,
        nr::WRITE,
        nr::PREAD64,
        nr::PWRITE64,
        nr::READV,
        nr::WRITEV,

        nr::CLOSE,
        nr::FCNTL,

        nr::EXIT_GROUP,        
        nr::EXIT,
        nr::RT_SIGRETURN,

        nr::FORK,
        nr::BRK,

        nr::ARCH_PRCTL,
        nr::PRCTL,

        // xxx really don't want to allow these but the tracer itself
        //   needs them, so they will need special bpf filters or we
        //   need to pare down the tracer further.
        nr::WAITID,
        nr::PTRACE,
        nr::EXECVE,
        nr::GETPID,
        nr::KILL,
        
    ], &[
        ret(SECCOMP_RET_ALLOW)
    ]);
    
    // Trace by default. This emulates the syscalls we emulate,
    // and others get logged in detail before we panic.
    p.inst(ret(SECCOMP_RET_TRACE));
}

pub fn activate() {
    let mut buffer = ProgramBuffer::new();
    filter(&mut buffer);
    println!("filter:\n{:?}", buffer); 
    let prog = buffer.to_filter_prog();
    let ptr = (&prog) as *const SockFilterProg as usize;
    let result = unsafe {
        syscall!(PRCTL, PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
        syscall!(PRCTL, PR_SET_SECCOMP, SECCOMP_MODE_FILTER, ptr, 0, 0) as isize
    };
    if result != 0 {
        panic!("seccomp setup error ({})", result);
    }
}
