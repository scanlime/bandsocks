use crate::{
    abi,
    abi::SyscallInfo,
    process::{syscall::SyscallEmulator, Event, EventSource, MessageSender},
    protocol::{FromSand, ProcessHandle, SysPid, ToSand, VPid},
    ptrace,
};
use core::fmt::{self, Debug, Formatter};

#[derive(Debug, Clone)]
pub struct TaskData {
    pub vpid: VPid,
    pub sys_pid: SysPid,
}

pub async fn task_fn(events: EventSource<'_>, msg: MessageSender<'_>, task_data: TaskData) {
    Task::new(events, msg, task_data).await.run().await;
}

pub struct Task<'q> {
    pub task_data: TaskData,
    pub process_handle: ProcessHandle,
    pub msg: MessageSender<'q>,
    pub events: EventSource<'q>,
}

impl<'q> Debug for Task<'q> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.task_data.fmt(f)
    }
}

impl<'q> Task<'q> {
    async fn new(
        mut events: EventSource<'q>,
        mut msg: MessageSender<'q>,
        task_data: TaskData,
    ) -> Task<'q> {
        ptrace::setoptions(task_data.sys_pid);
        assert_eq!(
            events.next().await,
            Event::Signal {
                sig: abi::SIGCHLD,
                code: abi::CLD_TRAPPED,
                status: abi::SIGSTOP,
            }
        );

        ptrace::cont(task_data.sys_pid);
        assert_eq!(
            events.next().await,
            Event::Signal {
                sig: abi::SIGCHLD,
                code: abi::CLD_TRAPPED,
                status: abi::PTRACE_SIG_EXEC,
            }
        );

        msg.send(FromSand::OpenProcess(task_data.sys_pid));
        match events.next().await {
            Event::Message(ToSand::OpenProcessReply(process_handle)) => Task {
                events,
                msg,
                process_handle,
                task_data,
            },
            other => panic!(
                "unexpected open_process reply, task={:x?}, received={:x?}",
                task_data, other
            ),
        }
    }

    async fn run(&mut self) {
        self.cont();
        loop {
            let event = self.events.next().await;
            match event {
                Event::Signal {
                    sig: abi::SIGCHLD,
                    code: abi::CLD_TRAPPED,
                    status: abi::PTRACE_SIG_SECCOMP,
                } => self.handle_seccomp_trap().await,

                Event::Signal {
                    sig: abi::SIGCHLD,
                    code: abi::CLD_TRAPPED,
                    status: signal,
                } if signal < 0x100 => self.handle_signal(signal).await,

                e => panic!("{:x?}, unexpected event, {:x?}", self.task_data, e),
            }
        }
    }

    fn cont(&self) {
        ptrace::cont(self.task_data.sys_pid);
    }

    async fn handle_signal(&mut self, signal: u32) {
        println!("sig {}", signal);
        self.cont();
    }

    async fn handle_seccomp_trap(&mut self) {
        let mut regs: abi::UserRegs = Default::default();
        ptrace::get_regs(self.task_data.sys_pid, &mut regs);

        let syscall_info = SyscallInfo {
            op: abi::PTRACE_SYSCALL_INFO_SECCOMP,
            arch: abi::AUDIT_ARCH_X86_64,
            instruction_pointer: regs.ip,
            stack_pointer: regs.sp,
            nr: regs.orig_ax,
            args: [regs.di, regs.si, regs.dx, regs.r10, regs.r8, regs.r9],
            ..Default::default()
        };

        let mut emulator = SyscallEmulator::new(self, &syscall_info);
        regs.ax = emulator.dispatch().await as u64;

        // Block the real system call from executing!
        regs.orig_ax = -1 as i64 as u64;
        ptrace::set_regs(self.task_data.sys_pid, &regs);
        self.cont();
    }
}
