// This code may not be used for any purpose. Be gay, do crime.

use core::mem;
use core::ptr::null;
use core::default::Default;
use sc::syscall;
use crate::abi;
use crate::process::{Process, SysPid, ProcessTable, State};

pub struct Tracer {
    process_table: ProcessTable,
}

unsafe fn be_the_child_process(cmd: &[u8], argv: &[*const u8], envp: &[*const u8]) -> ! {
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
    
impl Tracer {
    pub fn new() -> Self {
        Tracer {
            process_table: ProcessTable::new()
        }
    }

    pub fn spawn(&mut self, cmd: &[u8], argv: &[*const u8], envp: &[*const u8]) {
        unsafe { match syscall!(FORK) as isize {
            result if result == 0 => be_the_child_process(cmd, argv, envp),
            result if result < 0 => panic!("fork error"),
            result => self.expect_new_child(SysPid(result as u32)),
        }}
    }

    fn expect_new_child(&mut self, pid: SysPid) {
        if self.process_table.allocate(Process {
            sys_pid: pid,
            state: State::Spawning,
        }).is_err() {
            panic!("virtual process limit exceeded");
        }
    }

    fn handle_new_child(&mut self, pid: SysPid) {
        println!("new child, {:?}", pid);
    }

    fn handle_child_exit(&mut self, pid: SysPid, si_code: u32) {
        println!("child exit, {:?} code={}", pid, si_code);
    }
    
    pub fn handle_events(&mut self) {
        let mut info: abi::SigInfo = Default::default();
        let info_ptr = &mut info as *mut abi::SigInfo as usize;
        assert_eq!(mem::size_of_val(&info), abi::SI_MAX_SIZE);

        loop {
            let which = abi::P_ALL;
            let pid = -1 as isize as usize;
            let options = abi::WEXITED | abi::WSTOPPED | abi::WCONTINUED;
            let rusage = null::<usize>() as usize;
            let result = unsafe { syscall!(WAITID, which, pid, info_ptr, options, rusage) as isize };
            match result {
                0 => self.handle_event(&info),
                abi::ECHILD => {
                    // No more child processes
                    break;
                },
                other => {
                    panic!("waitid err ({})", other);
                }
            }
        }
    }

    fn handle_event(&mut self, info: &abi::SigInfo) {
        assert_eq!(info.si_signo, abi::SIGCHLD);
        let pid = SysPid(info.si_pid);
        match info.si_code {
            abi::CLD_EXITED | abi::CLD_KILLED | abi::CLD_DUMPED => {
                self.handle_child_exit(pid, info.si_code);
            },
            abi::CLD_STOPPED => {
                println!("stopped, {:?}", info);
                unsafe { syscall!(PTRACE, abi::PTRACE_CONT, pid.0, 0, 0); }
            }
            abi::CLD_TRAPPED => {
                println!("trapped, {:?}", info);
                if info.si_status == abi::SIGSTOP {
                    // 
                }
                unsafe { syscall!(PTRACE, abi::PTRACE_CONT, pid.0, 0, 0); }
            }
            abi::CLD_CONTINUED => {
                println!("cont, {:?}", info);
            }
            code => {
                panic!("unexpected siginfo, {}", code);
            }
        }
    }
}

