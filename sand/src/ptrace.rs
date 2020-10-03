// This code may not be used for any purpose. Be gay, do crime.

use core::mem;
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
        base: regs as *mut abi::UserRegs as *mut usize,
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
        base: regs as *const abi::UserRegs as *mut usize,
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

pub fn syscall_info(pid: SysPid, syscall_info: &mut abi::SyscallInfo) {
    let buf_size = span_of!(abi::SyscallInfo, ..ret_data).end;
    let ptr = syscall_info as *mut abi::SyscallInfo as usize;       
    match unsafe { syscall!(PTRACE, abi::PTRACE_GET_SYSCALL_INFO, pid.0, buf_size, ptr) as isize } {
        err if err < 0 => panic!("ptrace get syscall info failed ({})", err),
        actual_size if actual_size < buf_size as isize => {
            panic!("ptrace syscall info too short (kernel gave us {} bytes, expected {})",
                   actual_size, buf_size);
        },
        _ => (),
    }
}
