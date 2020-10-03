// This code may not be used for any purpose. Be gay, do crime.

use crate::bpf::*;
use crate::abi::*;
use sc::{syscall, nr};

fn filter(p: &mut ProgramBuffer) {
    // Examine syscall number
    p.inst(load(offset_of!(SeccompData, nr)));

    // Basic syscalls from seccomp 'strict' mode
    p.if_eq(nr::READ,         &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::WRITE,        &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::EXIT,         &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::RT_SIGRETURN, &[ ret(SECCOMP_RET_ALLOW) ]);
        
    // xxx everything below here is purely experimental

    // syscalls this binary currently needs
    // xxx: some of these do need sandboxing. either we have
    //      to remove functionality from this binary, or make
    //      sure tracing works in all of these cases, or do
    //      something else. (handle via signal or bpf)
    p.if_eq(nr::BRK,        &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::ARCH_PRCTL, &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::PRCTL,      &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::ACCESS,     &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::OPEN,       &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::OPENAT,     &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::CLOSE,      &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::FCNTL,      &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::MMAP,       &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::MPROTECT,   &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::FORK,       &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::EXECVE,     &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::WAITID,     &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::PTRACE,     &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::GETPID,     &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::PTRACE,     &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::KILL,       &[ ret(SECCOMP_RET_ALLOW) ]);
    p.if_eq(nr::GETPID,     &[ ret(SECCOMP_RET_ALLOW) ]);

    // temp: try emulating some things
    p.if_eq(nr::UNAME, &[
        imm(-1 as i32 as u32),
        store(offset_of!(SeccompData, nr)),
        ret(SECCOMP_RET_TRACE)
    ]);

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
