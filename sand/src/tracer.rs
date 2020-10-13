use core::mem;
use core::ptr::null;
use core::default::Default;
use core::convert::TryInto;
use sc::syscall;
use crate::abi;
use crate::abi::SyscallInfo;
use crate::emulator::SyscallEmulator;
use crate::process::{VPid, Process, ProcessTable, PID_LIMIT, State};
use crate::ipc::Socket;
use crate::protocol::{SysPid, MessageFromSand};
use crate::ptrace;

pub struct Tracer {
    ipc: Socket,
    process_table: ProcessTable,
}

impl Tracer {
    pub fn new(ipc: Socket) -> Self {

        // hi there ipc server
        ipc.send(&MessageFromSand::Nop(1,2));
        ipc.send(&MessageFromSand::Nop(3,4));

        Tracer {
            ipc,
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

    async fn handle_new_child(&mut self, pid: VPid, sys_pid: SysPid) {
        let mut process = self.process_table.get_mut(pid).unwrap();
        assert_eq!(sys_pid, process.sys_pid);
        println!("new child, {:?} {:?}", pid, process);
        ptrace::setoptions(process.sys_pid);
        process.state = State::Normal;
    }

    async fn handle_child_exit(&mut self, sys_pid: SysPid, si_code: u32) {
        println!("child exit, {:?} code={}", sys_pid, si_code);
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

    pub fn handle_events(&mut self) {

        let mut task_array = [ None; PID_LIMIT ];
        let mut siginfo: abi::SigInfo = Default::default();

        while ptrace::wait(&mut siginfo) {

            while let Some(message) = self.ipc.recv() {
                println!("received: {:?}", message);
            }

            let task = self.handle_event(&info);
            task_array[0] = Some(task);
        }
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
}
