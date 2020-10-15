use crate::process::{Event, EventSource};
use crate::process::syscall::SyscallEmulator;
use crate::protocol::{VPid, SysPid};
use crate::ptrace;

#[derive(Debug, Clone)]
pub struct TaskData {
    pub vpid: VPid,
    pub sys_pid: SysPid,
}

pub async fn task_fn<'a>(events: EventSource<'a>, task_data: TaskData) {
    Task { task_data, events }.run().await;
}

struct Task<'a> {
    task_data: TaskData,
    events: EventSource<'a>
}

impl<'a> Task<'a> {
    async fn run(&mut self) {

        println!("{:?} NEW", self.task_data);
        ptrace::setoptions(self.task_data.sys_pid);

        loop {
            let event = self.events.next().await;
            println!("{:?} {:?}", self.task_data, event);
        }

    }
}

        /*
        match event {
            Event::Signal(sig) if sig == abi::SIGCHLD =>

            assert_eq!(info.si_signo, abi::SIGCHLD);
        let sys_pid = SysPid(info.si_pid);
        match info.si_code {
            abi::CLD_STOPPED => panic!("unexpected 'stopped' state, {:?}", info),
            abi::CLD_CONTINUED => panic!("unexpected 'continued' state, {:?}", info),
            abi::CLD_EXITED | abi::CLD_KILLED | abi::CLD_DUMPED => self.handle_child_exit(sys_pid, info.si_code).await,
            abi::CLD_TRAPPED => self.handle_ptrace_trap(sys_pid, info.si_status).await,
            code => panic!("unexpected siginfo, {}", code),
        }


        fn handle_new_child(&mut self, pid: VPid, sys_pid: SysPid) {
        let mut process = self.process_table.get_mut(pid).unwrap();
        assert_eq!(sys_pid, process.sys_pid);
        println!("new child, {:?} {:?}", pid, process);
        ptrace::setoptions(process.sys_pid);
    }

    async fn handle_fork(&mut self, pid: VPid, sys_pid: SysPid) {
        let child = SysPid(ptrace::geteventmsg(sys_pid) as u32);
        println!("fork {:?} {:?} -> {:?}", pid, sys_pid, child);
        self.expect_new_child(child)
    }

    async fn handle_exec(&mut self, pid: VPid, sys_pid: SysPid) {
        println!("exec {:?} {:?}", pid, sys_pid);
    }

    async fn handle_signal(&mut self, pid: VPid, sys_pid: SysPid, signal: u8) {
        println!("signal {}, {:?} {:?}", signal, pid, sys_pid);
    }

    async fn handle_seccomp_trace(&mut self, pid: VPid, sys_pid: SysPid) {
        let mut regs: abi::UserRegs = Default::default();
        ptrace::get_regs(sys_pid, &mut regs);

        let syscall_info = SyscallInfo {
            op: abi::PTRACE_SYSCALL_INFO_SECCOMP,
            arch: abi::AUDIT_ARCH_X86_64,
            instruction_pointer: regs.ip,
            stack_pointer: regs.sp,
            nr: regs.orig_ax,
            args: [ regs.di, regs.si, regs.dx, regs.r10, regs.r8, regs.r9 ],
            ..Default::default()
        };

        let mut emulator = SyscallEmulator::new(pid, sys_pid, &syscall_info);
        regs.ax = emulator.dispatch().await as u64;

        // Block the real system call from executing!
        regs.orig_ax = -1 as i64 as u64;
        ptrace::set_regs(sys_pid, &mut regs);
    }

    async fn handle_event(&mut self, info: &abi::SigInfo) {
        assert_eq!(info.si_signo, abi::SIGCHLD);
        let sys_pid = SysPid(info.si_pid);
        match info.si_code {
            abi::CLD_STOPPED => panic!("unexpected 'stopped' state, {:?}", info),
            abi::CLD_CONTINUED => panic!("unexpected 'continued' state, {:?}", info),
            abi::CLD_EXITED | abi::CLD_KILLED | abi::CLD_DUMPED => self.handle_child_exit(sys_pid, info.si_code).await,
            abi::CLD_TRAPPED => self.handle_ptrace_trap(sys_pid, info.si_status).await,
            code => panic!("unexpected siginfo, {}", code),
        }
    }

    async fn handle_ptrace_trap(&mut self, sys_pid: SysPid, signal: u32) {
        let (pid, process) = match self.process_table.find_sys_pid(sys_pid) {
            None => panic!("ptrace trap from unknown {:?}", sys_pid),
            Some(result) => result
        };
        match process.state {
            State::Spawning => {
                match signal {
                    abi::SIGSTOP => self.handle_new_child(pid, sys_pid).await,
                    _ => panic!("unexpected signal {} during process startup", signal),
                }
            },
            State::Normal => {
                match signal {
                    abi::PTRACE_SIG_FORK => self.handle_fork(pid, sys_pid).await,
                    abi::PTRACE_SIG_EXEC => self.handle_exec(pid, sys_pid).await,
                    abi::PTRACE_SIG_VFORK => panic!("unhandled vfork"),
                    abi::PTRACE_SIG_CLONE => panic!("unhandled clone"),
                    abi::PTRACE_SIG_VFORK_DONE => panic!("unhandled vfork_done"),
                    abi::PTRACE_SIG_SECCOMP => self.handle_seccomp_trace(pid, sys_pid).await,
                    signal if signal < 0x100 => self.handle_signal(pid, sys_pid, signal.try_into().unwrap()).await,
                    other => panic!("unhandled trap 0x{:x}, {:?} {:?}", other, pid, process),
                }
            },
        }
        ptrace::cont(sys_pid);
    }
    */
