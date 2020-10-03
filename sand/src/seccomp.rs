// This code may not be used for any purpose. Be gay, do crime.

use crate::bpf::*;
use crate::abi::*;
use sc::{syscall, nr};

fn filter(p: &mut ProgramBuffer) {
    // Examine syscall number
    p.inst(load(offset_of!(SeccompData, nr)));

    // Basic syscalls from seccomp 'strict' mode
    p.if_any_eq(&[
        nr::READ,
        nr::WRITE,
        nr::EXIT,
        nr::RT_SIGRETURN,
    ], &[
        ret(SECCOMP_RET_ALLOW)
    ]);
        
    // xxx everything below here is purely experimental

    // syscalls this binary currently needs
    // xxx: some of these do need sandboxing. either we have
    //      to remove functionality from this binary, or make
    //      sure tracing works in all of these cases, or do
    //      something else. (handle via signal or bpf)
    p.if_any_eq(&[
        nr::BRK,
        nr::ARCH_PRCTL,
        nr::PRCTL,
        nr::ACCESS,
        nr::OPEN,
        nr::OPENAT,
        nr::CLOSE,
        nr::FCNTL,
        nr::MMAP,
        nr::MPROTECT,
        nr::FORK,
        nr::EXECVE,
        nr::WAITID,
        nr::PTRACE,
        nr::GETPID,
        nr::PTRACE,
        nr::KILL,
        nr::GETPID,
    ], &[
        ret(SECCOMP_RET_ALLOW)
    ]);
    
    // temp: try emulating some things
    p.if_eq(nr::UNAME, &[
        imm(-1 as i32 as u32),
        ret(SECCOMP_RET_TRACE)
    ]);

    // xxx
    p.inst(ret(SECCOMP_RET_ALLOW));
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
