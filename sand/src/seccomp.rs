use sc::nr;
use seccomp_tiny::{abi::*, bpf::*, ProgramBuffer};

// This file has two policies; the "tracer" policy is applied very early, and
// covers this process for its entire lifetime. The "loader" policy is applied
// during stage 2, and it applies additional ruless which the sandbox contents
// use but not the tracer.
//
// For comparison, the container we might be running in likely has a policy like
// this one: https://github.com/moby/moby/blob/master/profiles/seccomp/default.json

fn base_rules_for_all_policies() -> ProgramBuffer {
    let mut p = ProgramBuffer::new();

    // Keep syscall in the accumulator generally
    p.inst(load(offset_of!(SeccompData, nr)));

    // Fully allowed in all modes
    // to do: none of this has been audited yet. this will generally be all syscalls
    // that deal with existing fds or with memory, but nothing that deals with pids
    // and nothing that has a pathname in it.
    // to do: explicitly whitelist constants on functions like seek and mmap
    p.if_any_eq(
        &[
            nr::READ,
            nr::WRITE,
            nr::PREAD64,
            nr::PWRITE64,
            nr::READV,
            nr::WRITEV,
            nr::LSEEK,
            nr::SOCKETPAIR,
            nr::EXIT_GROUP,
            nr::EXIT,
            nr::COPY_FILE_RANGE,
            nr::DUP2,
            nr::RT_SIGPROCMASK,
            nr::RT_SIGACTION,
            nr::RT_SIGRETURN,
            nr::SIGALTSTACK,
            nr::SENDFILE,
            nr::MMAP,
            nr::TIME,
            nr::FUTEX,
            nr::SELECT,
            nr::POLL,
            nr::MPROTECT,
            nr::MUNMAP,
            nr::MREMAP,
            nr::NANOSLEEP,
            nr::GETRANDOM,
            nr::MEMFD_CREATE,
            nr::SET_ROBUST_LIST,
            nr::GETRLIMIT,
            // fixme: only allow some operations
            nr::FCNTL,
            nr::ARCH_PRCTL,
            nr::PRCTL,
            // fixme: only allow pid==0 case
            nr::SCHED_GETAFFINITY,
            nr::PRLIMIT64,
        ],
        &[ret(SECCOMP_RET_ALLOW)],
    );
    p
}

pub fn policy_for_tracer() {
    let mut p = base_rules_for_all_policies();

    // these are emulated inside the sandbox, but the tracer is allowed to use them
    // to do: none of this has been audited yet
    p.if_any_eq(
        &[
            nr::SENDMSG,
            nr::RECVMSG,
            nr::CLOSE,
            nr::WAITID,
            nr::PTRACE,
            nr::GETPID,
            nr::RT_SIGACTION,
            nr::RT_SIGRETURN,
            // need this to get to the next stage
            // xxx: drop this privilege as soon as we initialize the tracer
            nr::FORK,
            nr::EXECVE,
            // xxx: can't allow this, use a different attach mechanism?
            nr::KILL,
        ],
        &[ret(SECCOMP_RET_ALLOW)],
    );

    // There is no tracer yet, but we want to allow tracing later.
    // With no tracer attached this blocks the syscall with ENOSYS.
    p.inst(ret(SECCOMP_RET_TRACE));

    p.activate();
}

pub fn policy_for_loader() {
    let mut p = base_rules_for_all_policies();

    // Calls to emulate / calls to allow the emulator to remotely issue
    p.if_any_eq(
        &[
            nr::ACCESS,
            nr::BRK,
            nr::CHDIR,
            nr::CLOSE,
            nr::EXECVE,
            nr::FSTAT,
            nr::FORK,
            nr::CLONE,
            nr::GETCWD,
            nr::GETDENTS64,
            nr::GETEGID,
            nr::GETEUID,
            nr::GETGID,
            nr::GETPGRP,
            nr::GETPID,
            nr::GETPPID,
            nr::GETPGID,
            nr::GETUID,
            nr::GETTID,
            nr::IOCTL,
            nr::LSTAT,
            nr::NEWFSTATAT,
            nr::STATFS,
            nr::FSTATFS,
            nr::OPEN,
            nr::OPENAT,
            nr::RECVMSG,
            nr::SENDMSG,
            nr::SETPGID,
            nr::SET_TID_ADDRESS,
            nr::STAT,
            nr::SYSINFO,
            nr::UNAME,
        ],
        &[ret(SECCOMP_RET_TRACE)],
    );

    // All other syscalls panic via SIGSYS
    p.inst(ret(SECCOMP_RET_TRAP));

    p.activate();
}
