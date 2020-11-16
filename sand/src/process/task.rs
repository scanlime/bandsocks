use crate::{
    abi,
    abi::{SyscallInfo, UserRegs},
    nolibc::{fcntl, socketpair},
    process::{remote::RemoteFd, syscall::SyscallEmulator, Event, EventSource, MessageSender},
    protocol::{FromTask, ProcessHandle, SysFd, SysPid, ToTask, VPid},
    ptrace,
};
use core::fmt::{self, Debug, Formatter};

#[derive(Debug, Clone)]
pub struct TaskSocketPair {
    pub tracer: SysFd,
    pub remote: RemoteFd,
}

#[derive(Debug, Clone)]
pub struct TaskData {
    pub vpid: VPid,
    pub sys_pid: SysPid,
    pub parent: Option<VPid>,
    pub socket_pair: TaskSocketPair,
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

#[derive(Debug)]
pub struct StoppedTask<'q, 's> {
    pub task: &'s mut Task<'q>,
    pub regs: &'s mut UserRegs,
}

impl TaskSocketPair {
    pub fn new_inheritable() -> Self {
        let (tracer, remote) =
            socketpair(abi::AF_UNIX, abi::SOCK_STREAM, 0).expect("task socket pair");
        fcntl(&tracer, abi::F_SETFD, abi::F_CLOEXEC).expect("task socket fcntl");
        fcntl(&remote, abi::F_SETFD, 0).expect("task socket fcntl");
        // The file will be inherited
        let remote = RemoteFd(remote.0);
        TaskSocketPair { tracer, remote }
    }
}

impl<'q> Debug for Task<'q> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Task")
            .field(&self.task_data)
            .field(&self.process_handle)
            .finish()
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

        msg.send(FromTask::OpenProcess(task_data.sys_pid));
        match events.next().await {
            Event::Message(ToTask::OpenProcessReply(process_handle)) => Task {
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
                    status: abi::PTRACE_SIG_FORK,
                } => {
                    let child_pid = ptrace::geteventmsg(self.task_data.sys_pid) as u32;
                    self.handle_fork(child_pid).await
                }

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

                e => panic!("{:?}, unexpected event, {:?}", self.task_data, e),
            }
        }
    }

    fn cont(&self) {
        ptrace::cont(self.task_data.sys_pid);
    }

    async fn handle_signal(&mut self, signal: u32) {
        let mut regs: abi::UserRegs = Default::default();
        ptrace::get_regs(self.task_data.sys_pid, &mut regs);
        panic!("signal {}, {:x?}", signal, regs);
    }

    async fn handle_fork(&mut self, child_pid: u32) {
        panic!("fork not handled yet, pid {}", child_pid);
    }

    async fn handle_seccomp_trap(&mut self) {
        let mut regs: abi::UserRegs = Default::default();
        ptrace::get_regs(self.task_data.sys_pid, &mut regs);
        let mut stopped_task = StoppedTask {
            task: self,
            regs: &mut regs,
        };
        SyscallEmulator::new(&mut stopped_task).dispatch().await;
        SyscallInfo::orig_nr_to_regs(abi::SYSCALL_BLOCKED, &mut regs);
        ptrace::set_regs(self.task_data.sys_pid, &regs);
        self.cont();
    }
}
