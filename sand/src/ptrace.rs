// This code may not be used for any purpose. Be gay, do crime.

use core::fmt;
use sc::syscall;
use crate::abi;
use crate::process::SysPid;

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

pub fn geteventmsg(pid: SysPid) -> usize {
    let mut result : usize = -1 as isize as usize;
    match unsafe { syscall!(PTRACE, abi::PTRACE_GETEVENTMSG, pid.0, 0, &mut result as *mut usize) as isize } {
        0 => result,
        err => panic!("ptrace geteventmsg failed ({})", err)
    }
}

pub fn syscall_info(pid: SysPid, syscall_info: &mut abi::PTraceSyscallInfo) {
    let buf_size = span_of!(abi::PTraceSyscallInfo, ..ret_data).end;
    let ptr = syscall_info as *mut abi::PTraceSyscallInfo as usize;       
    match unsafe { syscall!(PTRACE, abi::PTRACE_GET_SYSCALL_INFO, pid.0, buf_size, ptr) as isize } {
        err if err < 0 => panic!("ptrace get syscall info failed ({})", err),
        actual_size if actual_size < buf_size as isize => {
            panic!("ptrace syscall info too short (kernel gave us {} bytes, expected {})",
                   actual_size, buf_size);
        },
        _ => (),
    }
}

impl fmt::Debug for abi::PTraceSyscallInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SYS_{} {:?} {{ ip={:x} sp={:x} ret={:x} arch={:x} op={} }}",
               self.nr,
               self.args,
               self.instruction_pointer,
               self.stack_pointer,
               self.ret_data,
               self.arch,
               self.op)
    }
}
