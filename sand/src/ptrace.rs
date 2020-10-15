use core::mem;
use core::ptr::null;
use sc::syscall;
use crate::abi;
use crate::protocol::SysPid;

pub unsafe fn be_the_child_process(cmd: &[u8], argv: &[*const u8], envp: &[*const u8]) -> ! {
    // Make attachable, but doesn't wait for the tracer
    match syscall!(PTRACE, abi::PTRACE_TRACEME, 0, 0, 0) as isize {
        0 => {},
        result => panic!("ptrace error, {}", result)
    }

    // Let the tracer attach before we exec
    syscall!(KILL, syscall!(GETPID), abi::SIGSTOP);

    let result = syscall!(EXECVE, cmd.as_ptr(), argv.as_ptr(), envp.as_ptr()) as isize;
    panic!("exec failed, {}", result);
}

pub fn cont(pid: SysPid) {
    unsafe { syscall!(PTRACE, abi::PTRACE_CONT, pid.0, 0, 0); }
}

pub fn setoptions(pid: SysPid) {
    let options =
          abi::PTRACE_O_EXITKILL
        | abi::PTRACE_O_TRACECLONE
        | abi::PTRACE_O_TRACEEXEC
        | abi::PTRACE_O_TRACEFORK
        | abi::PTRACE_O_TRACESYSGOOD
        | abi::PTRACE_O_TRACEVFORK
        | abi::PTRACE_O_TRACEVFORK_DONE
        | abi::PTRACE_O_TRACESECCOMP;
    unsafe {
        syscall!(PTRACE, abi::PTRACE_SETOPTIONS, pid.0, 0, options);
    }
}

pub fn get_regs(pid: SysPid, regs: &mut abi::UserRegs) {
    let mut iovec = abi::IOVec {
        base: regs as *mut abi::UserRegs as *mut u8,
        len: mem::size_of_val(regs)
    };
    match unsafe { syscall!(PTRACE, abi::PTRACE_GETREGSET, pid.0,
                            abi::NT_PRSTATUS, &mut iovec as *mut abi::IOVec) as isize } {
        0 => (),
        err => panic!("ptrace getregset failed ({})", err)
    }
    assert_eq!(iovec.len, mem::size_of_val(regs));
}

pub fn set_regs(pid: SysPid, regs: &abi::UserRegs) {
    let mut iovec = abi::IOVec {
        base: regs as *const abi::UserRegs as *mut u8,
        len: mem::size_of_val(regs)
    };
    match unsafe { syscall!(PTRACE, abi::PTRACE_SETREGSET, pid.0,
                            abi::NT_PRSTATUS, &mut iovec as *mut abi::IOVec) as isize } {
        0 => (),
        err => panic!("ptrace getregset failed ({})", err)
    }
    assert_eq!(iovec.len, mem::size_of_val(regs));
}

pub fn geteventmsg(pid: SysPid) -> usize {
    let mut result : usize = -1 as isize as usize;
    match unsafe { syscall!(PTRACE, abi::PTRACE_GETEVENTMSG, pid.0, 0, &mut result as *mut usize) as isize } {
        0 => result,
        err => panic!("ptrace geteventmsg failed ({})", err)
    }
}

pub fn wait(info: &mut abi::SigInfo) -> isize {
    let info_ptr = info as *mut abi::SigInfo as usize;
    assert_eq!(mem::size_of_val(&info), abi::SI_MAX_SIZE);
    let which = abi::P_ALL;
    let pid = -1 as isize as usize;
    let options = abi::WEXITED | abi::WSTOPPED | abi::WCONTINUED;
    let rusage = null::<usize>() as usize;
    unsafe { syscall!(WAITID, which, pid, info_ptr, options, rusage) as isize }
}
