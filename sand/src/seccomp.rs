use crate::abi;
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
            nr::COPY_FILE_RANGE,
            nr::EXIT,
            nr::EXIT_GROUP,
            nr::FUTEX,
            nr::GETRANDOM,
            nr::GETRLIMIT,
            nr::LSEEK,
            nr::MEMFD_CREATE,
            nr::MMAP,
            nr::MPROTECT,
            nr::MREMAP,
            nr::MUNMAP,
            nr::NANOSLEEP,
            nr::POLL,
            nr::PREAD64,
            nr::PSELECT6,
            nr::PWRITE64,
            nr::READ,
            nr::READV,
            nr::RT_SIGACTION,
            nr::RT_SIGPROCMASK,
            nr::RT_SIGRETURN,
            nr::SELECT,
            nr::SENDFILE,
            nr::SET_ROBUST_LIST,
            nr::SIGALTSTACK,
            nr::TIME,
            nr::WRITE,
            nr::WRITEV,
            // fixme: only allow some operations
            nr::FCNTL,
            nr::ARCH_PRCTL,
            nr::PRCTL,
            nr::FADVISE64,
            // fixme: only allow pid==0 case
            nr::SCHED_GETAFFINITY,
            nr::PRLIMIT64,
        ],
        &[ret(SECCOMP_RET_ALLOW)],
    );
    p
}

pub fn base_rules_for_tracer() -> ProgramBuffer {
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
            nr::SOCKETPAIR,
        ],
        &[ret(SECCOMP_RET_ALLOW)],
    );

    p
}

pub fn policy_for_tracer_init() {
    let mut p = base_rules_for_tracer();

    // During init, we need the tracer to make one real non-emulated fork and exec,
    // which will subsequently be disallowed/emulated.
    p.if_any_eq(&[nr::FORK, nr::EXECVE], &[ret(SECCOMP_RET_ALLOW)]);

    // There is no tracer yet, but we want to allow tracing later.
    // With no tracer attached this blocks the syscall with ENOSYS.
    p.inst(ret(SECCOMP_RET_TRACE));

    p.activate();
}

pub fn policy_for_tracer_after_init() {
    let mut p = base_rules_for_tracer();

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
            nr::CLONE,
            nr::CLOSE,
            nr::DUP,
            nr::DUP2,
            nr::EXECVE,
            nr::FCHDIR,
            nr::FORK,
            nr::FSTAT,
            nr::FSTATFS,
            nr::GETCWD,
            nr::GETDENTS64,
            nr::GETEGID,
            nr::GETEUID,
            nr::GETGID,
            nr::GETPGID,
            nr::GETPGRP,
            nr::GETPID,
            nr::GETPPID,
            nr::GETTID,
            nr::GETUID,
            nr::IOCTL,
            nr::LSTAT,
            nr::NEWFSTATAT,
            nr::OPEN,
            nr::OPENAT,
            nr::READLINK,
            nr::RECVMSG,
            nr::SENDMSG,
            nr::SETPGID,
            nr::SET_TID_ADDRESS,
            nr::STAT,
            nr::STATFS,
            nr::SYSINFO,
            nr::UNAME,
            nr::WAIT4,
        ],
        &[ret(SECCOMP_RET_TRACE)],
    );

    // Reject network subsystem
    p.if_any_eq(
        &[
            nr::SOCKET,
            nr::BIND,
            nr::LISTEN,
            nr::CONNECT,
            nr::ACCEPT,
            nr::SHUTDOWN,
            nr::GETSOCKNAME,
            nr::GETPEERNAME,
            nr::SOCKETPAIR,
            nr::SETSOCKOPT,
            nr::GETSOCKOPT,
        ],
        &[ret(SECCOMP_RET_ERRNO | -abi::ENOSYS as u16 as u32)],
    );

    // Reject filesystem modification
    p.if_any_eq(
        &[
            nr::MKDIR,
            nr::RMDIR,
            nr::CHMOD,
            nr::CREAT,
            nr::LINK,
            nr::UNLINK,
            nr::SYMLINK,
            nr::CHMOD,
            nr::FCHMOD,
            nr::CHOWN,
            nr::FCHOWN,
        ],
        &[ret(SECCOMP_RET_ERRNO | -abi::EROFS as u16 as u32)],
    );

    // All other syscalls panic via SIGSYS
    p.inst(ret(SECCOMP_RET_TRAP));

    p.activate();
}
