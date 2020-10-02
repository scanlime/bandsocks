// This code may not be used for any purpose. Be gay, do crime.

use core::mem;
use core::ptr::null;
use core::default::Default;
use sc::syscall;
use crate::abi;
use crate::process::{Process, VPid, SysPid, ProcessTable, State};

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

fn ptrace_continue(pid: SysPid) {
    unsafe { syscall!(PTRACE, abi::PTRACE_CONT, pid.0, 0, 0); }
}

fn ptrace_setoptions(pid: SysPid) {
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

fn ptrace_geteventmsg(pid: SysPid) -> usize {
    let mut result : usize = -1 as isize as usize;
    match unsafe { syscall!(PTRACE, abi::PTRACE_GETEVENTMSG, pid.0, 0, &mut result as *mut usize) as isize } {
        0 => result,
        err => panic!("ptrace geteventmsg failed ({})", err)
    }
}

fn ptrace_syscall_info(pid: SysPid, syscall_info: &mut abi::PTraceSyscallInfo) {
    let buf_size = mem::size_of_val(syscall_info);
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

    fn expect_new_child(&mut self, sys_pid: SysPid) {
        if self.process_table.allocate(Process {
            sys_pid,
            state: State::Spawning,
        }).is_err() {
            panic!("virtual process limit exceeded");
        }
    }

    fn handle_new_child(&mut self, pid: VPid, sys_pid: SysPid) {
        let mut process = self.process_table.get_mut(pid).unwrap();
        assert_eq!(sys_pid, process.sys_pid);
        println!("new child, {:?} {:?}", pid, process);
        ptrace_setoptions(process.sys_pid);
        process.state = State::Normal;
    }

    fn handle_child_exit(&mut self, sys_pid: SysPid, si_code: u32) {
        println!("child exit, {:?} code={}", sys_pid, si_code);
    }

    fn handle_child_reaped(&mut self, pid: VPid, sys_pid: SysPid) {
        println!("child reaped, {:?} {:?}", pid, sys_pid);
        self.process_table.free(pid);
    }
    
    fn handle_fork(&mut self, pid: VPid, sys_pid: SysPid) {
        let child = SysPid(ptrace_geteventmsg(sys_pid) as u32);
        println!("fork {:?} -> {:?}", sys_pid, child);
        self.expect_new_child(child)
    }
 
    fn handle_exec(&mut self, pid: VPid, sys_pid: SysPid) {
        println!("exec {:?} {:?}", pid, sys_pid);
    }

    fn handle_seccomp_trace(&mut self, pid: VPid, sys_pid: SysPid) {
        let mut syscall_info: abi::PTraceSyscallInfo = Default::default();
        ptrace_syscall_info(sys_pid, &mut syscall_info);
        println!("seccomp trace {:?} {:?} {:?}", pid, sys_pid, syscall_info);
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
                abi::ECHILD => break,
                other => panic!("waitid err ({})", other),
            }
        }
    }

    fn handle_event(&mut self, info: &abi::SigInfo) {
        assert_eq!(info.si_signo, abi::SIGCHLD);
        let sys_pid = SysPid(info.si_pid);
        match info.si_code {
            abi::CLD_EXITED | abi::CLD_KILLED | abi::CLD_DUMPED => self.handle_child_exit(sys_pid, info.si_code),
            abi::CLD_STOPPED => println!("stopped, {:?}", info),
            abi::CLD_CONTINUED => println!("cont, {:?}", info),
            abi::CLD_TRAPPED => self.handle_ptrace_trap(sys_pid, info.si_status),
            code => panic!("unexpected siginfo, {}", code),
        }
    }

    fn handle_ptrace_trap(&mut self, sys_pid: SysPid, signal: u32) {
        let (pid, process) = match self.process_table.find_sys_pid(sys_pid) {
            None => panic!("ptrace trap from unknown {:?}", sys_pid),
            Some(result) => result
        };
        match process.state {
            State::Spawning => {
                match signal {
                    abi::SIGSTOP => self.handle_new_child(pid, sys_pid),
                    _ => panic!("unexpected signal {} during process startup", signal),
                }
            },
            State::Normal => {
                match signal {
                    abi::PTRACE_SIG_FORK => self.handle_fork(pid, sys_pid),
                    abi::PTRACE_SIG_VFORK => panic!("unhandled vfork"),
                    abi::PTRACE_SIG_CLONE => panic!("unhandled clone"),
                    abi::PTRACE_SIG_EXEC => self.handle_exec(pid, sys_pid),
                    abi::PTRACE_SIG_VFORK_DONE => panic!("unhandled vfork_done"),
                    abi::PTRACE_SIG_SECCOMP => self.handle_seccomp_trace(pid, sys_pid),
                    other => println!("unhandled trap 0x{:x}, {:?} {:?}", signal, pid, process),
                }
            },
        }
        ptrace_continue(sys_pid);
    }
}

