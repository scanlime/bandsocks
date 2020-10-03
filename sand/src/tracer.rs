// This code may not be used for any purpose. Be gay, do crime.

use core::mem;
use core::ptr::null;
use core::default::Default;
use core::convert::TryInto;
use sc::{syscall, nr};
use crate::abi;
use crate::abi::SyscallInfo;
use crate::process::{Process, VPid, SysPid, ProcessTable, State};
use crate::ptrace;

pub struct Tracer {
    process_table: ProcessTable,
}

impl Tracer {
    pub fn new() -> Self {
        Tracer {
            process_table: ProcessTable::new()
        }
    }

    pub fn spawn(&mut self, cmd: &[u8], argv: &[*const u8], envp: &[*const u8]) {
        unsafe { match syscall!(FORK) as isize {
            result if result == 0 => ptrace::be_the_child_process(cmd, argv, envp),
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
        ptrace::setoptions(process.sys_pid);
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
        let child = SysPid(ptrace::geteventmsg(sys_pid) as u32);
        println!("fork {:?} {:?} -> {:?}", pid, sys_pid, child);
        self.expect_new_child(child)
    }
 
    fn handle_exec(&mut self, pid: VPid, sys_pid: SysPid) {
        println!("exec {:?} {:?}", pid, sys_pid);
    }

    fn handle_signal(&mut self, pid: VPid, sys_pid: SysPid, signal: u8) {
        println!("signal {}, {:?} {:?}", signal, pid, sys_pid);
        // to do: reap child in our own PID table after SIGCHLD
        if signal as u32 == abi::SIGSEGV {
            panic!("segmentation fault");
        }
    }

    fn handle_seccomp_trace(&mut self, pid: VPid, sys_pid: SysPid) {
        let mut syscall_info: SyscallInfo = Default::default();
        let mut regs: abi::UserRegs = Default::default();

        // All the information we need is in 'regs', but get syscall_info too and cross-check.
        ptrace::syscall_info(sys_pid, &mut syscall_info);
        ptrace::get_regs(sys_pid, &mut regs);

        assert_eq!(syscall_info.op, abi::PTRACE_SYSCALL_INFO_SECCOMP);
        assert_eq!(syscall_info.pad0, 0);
        assert_eq!(syscall_info.pad1, 0);
        assert_eq!(syscall_info.arch, abi::AUDIT_ARCH_X86_64);
        assert_eq!(syscall_info.instruction_pointer, regs.ip);
        assert_eq!(syscall_info.stack_pointer, regs.sp);
        assert_eq!(syscall_info.nr, regs.orig_ax);
        assert_eq!(syscall_info.args[0], regs.di);
        assert_eq!(syscall_info.args[1], regs.si);
        assert_eq!(syscall_info.args[2], regs.dx);
        assert_eq!(syscall_info.args[3], regs.r10);
        assert_eq!(syscall_info.args[4], regs.r8);
        assert_eq!(syscall_info.args[5], regs.r9);

        // Emulate the system call; this can make additional ptrace calls to read/write memory.
        regs.ax = self.emulate_syscall(pid, sys_pid, &syscall_info) as u64;
        
        // Block the real system call from executing!
        regs.orig_ax = -1 as i64 as u64;
        ptrace::set_regs(sys_pid, &mut regs);
    }

    fn emulate_syscall(&mut self, pid: VPid, sys_pid: SysPid, syscall_info: &SyscallInfo) -> isize {
        match syscall_info.nr {
            nr::ACCESS => self.emulate_access(pid, sys_pid, syscall_info),
            nr::OPENAT => self.emulate_openat(pid, sys_pid, syscall_info),
            nr::UNAME => self.emulate_uname(pid, sys_pid, syscall_info),
            nr::STAT => self.emulate_stat(pid, sys_pid, syscall_info),
            nr::MMAP => self.emulate_mmap(pid, sys_pid, syscall_info),
            nr::FSTAT => self.emulate_fstat(pid, sys_pid, syscall_info),
            other => panic!("unexpected syscall trace, SYS_{} {:?} {:?} {:?}", other, syscall_info, pid, sys_pid)
        }
    }

    fn emulate_access(&mut self, pid: VPid, sys_pid: SysPid, syscall_info: &SyscallInfo) -> isize {
        println!("ACCESS IS NOT HAPPENING");
        regs.ax = 0;
    }

    fn emulate_openat(&mut self, pid: VPid, sys_pid: SysPid, syscall_info: &SyscallInfo) -> isize {
        println!("OPENAT NOPE");
        regs.ax = 0;
    }

    fn emulate_uname(&mut self, pid: VPid, sys_pid: SysPid, syscall_info: &SyscallInfo) -> isize {
        println!("FAKE UNAME A COMIN");
        regs.ax = 0;
    }

    fn emulate_stat(&mut self, pid: VPid, sys_pid: SysPid, syscall_info: &SyscallInfo) -> isize {
        println!("GET ME A FILESYSTEM STAT");
        regs.ax = 0;
    }

    fn emulate_fstat(&mut self, pid: VPid, sys_pid: SysPid, syscall_info: &SyscallInfo) -> isize {
        println!("GOT NO TIME TO FSTAT");
        regs.ax = 0;
    }

    fn emulate_mmap(&mut self, pid: VPid, sys_pid: SysPid, syscall_info: &SyscallInfo) -> isize {
        println!("ARE WE REALLY EMULATING MMAP THO");
        regs.ax = 0;
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
            abi::CLD_STOPPED => panic!("unexpected 'stopped' state, {:?}", info),
            abi::CLD_CONTINUED => panic!("unexpected 'continued' state, {:?}", info),
            abi::CLD_EXITED | abi::CLD_KILLED | abi::CLD_DUMPED => self.handle_child_exit(sys_pid, info.si_code),
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
                    abi::PTRACE_SIG_EXEC => self.handle_exec(pid, sys_pid),
                    abi::PTRACE_SIG_VFORK => panic!("unhandled vfork"),
                    abi::PTRACE_SIG_CLONE => panic!("unhandled clone"),
                    abi::PTRACE_SIG_VFORK_DONE => panic!("unhandled vfork_done"),
                    abi::PTRACE_SIG_SECCOMP => self.handle_seccomp_trace(pid, sys_pid),
                    signal if signal < 0x100 => self.handle_signal(pid, sys_pid, signal.try_into().unwrap()),
                    other => panic!("unhandled trap 0x{:x}, {:?} {:?}", other, pid, process),
                }
            },
        }
        ptrace::cont(sys_pid);
    }
}
