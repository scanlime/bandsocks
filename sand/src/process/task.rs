use crate::{
    abi,
    abi::{SyscallInfo, UserRegs},
    nolibc::{fcntl, socketpair},
    process::{maps::print_maps_dump, syscall::SyscallEmulator, Event, EventSource, MessageSender},
    protocol::{FromTask, LogLevel, LogMessage, ProcessHandle, SysFd, SysPid, ToTask, VPid, VPtr},
    ptrace,
    remote::{mem::print_stack_dump, RemoteFd},
    tracer::TracerSettings,
};
use core::fmt::{self, Debug, Formatter};

#[derive(Debug, Clone)]
pub struct TaskSocketPair {
    pub tracer: SysFd,
    pub remote: RemoteFd,
}

#[derive(Debug, Clone)]
pub struct TaskMemManagement {
    // brk is emulated, since the real kernel's brk_start can't be changed without privileges
    pub brk: VPtr,
    pub brk_start: VPtr,
}

#[derive(Debug, Clone)]
pub struct TaskData {
    pub vpid: VPid,
    pub sys_pid: SysPid,
    pub parent: Option<VPid>,
    pub socket_pair: TaskSocketPair,
    pub mm: TaskMemManagement,
    pub tracer_settings: TracerSettings,
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
        Task::expect_event_or_panic(
            &mut events,
            task_data.sys_pid,
            Event::Signal {
                sig: abi::SIGCHLD,
                code: abi::CLD_TRAPPED,
                status: abi::SIGSTOP,
            },
        )
        .await;

        ptrace::cont(task_data.sys_pid);
        Task::expect_event_or_panic(
            &mut events,
            task_data.sys_pid,
            Event::Signal {
                sig: abi::SIGCHLD,
                code: abi::CLD_TRAPPED,
                status: abi::PTRACE_SIG_EXEC,
            },
        )
        .await;

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

    pub fn log_enabled(&self, level: LogLevel) -> bool {
        level <= self.task_data.tracer_settings.max_log_level
    }

    pub fn log(&mut self, level: LogLevel, message: LogMessage) {
        if self.log_enabled(level) {
            self.msg.send(FromTask::Log(level, message));
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

                Event::Signal {
                    sig: abi::SIGCHLD,
                    code: abi::CLD_EXITED,
                    status,
                } => return self.handle_exited(status).await,

                e => panic!("{:?}, unexpected event, {:?}", self.task_data, e),
            }
        }
    }

    pub async fn expect_event_or_panic(
        events: &mut EventSource<'q>,
        sys_pid: SysPid,
        expected: Event,
    ) {
        let received = events.next().await;
        if received != expected {
            let mut regs: UserRegs = Default::default();
            ptrace::get_regs(sys_pid, &mut regs);
            panic!(
                "*** unexpected event ***\nexpected: {:?}\nreceived: {:?}\ntask: {:?} {:x?}",
                expected, received, sys_pid, regs
            );
        }
    }

    fn cont(&self) {
        ptrace::cont(self.task_data.sys_pid);
    }

    fn as_stopped_task<'s>(&'s mut self, regs: &'s mut UserRegs) -> StoppedTask<'q, 's> {
        ptrace::get_regs(self.task_data.sys_pid, regs);
        StoppedTask { task: self, regs }
    }

    async fn handle_signal(&mut self, signal: u32) {
        // to do
        let mut regs: UserRegs = Default::default();
        let mut stopped_task = self.as_stopped_task(&mut regs);
        print_stack_dump(&mut stopped_task);
        print_maps_dump(&mut stopped_task);
        println!("task state:\n{:x?}\n", stopped_task.regs);
        panic!("*** signal {} inside sandbox ***", signal);
    }

    async fn handle_fork(&mut self, child_pid: u32) {
        panic!("fork not handled yet, pid {}", child_pid);
    }

    async fn handle_exited(&mut self, exit_code: u32) {
        self.msg.send(FromTask::Exited(exit_code as i32));
    }

    async fn handle_seccomp_trap(&mut self) {
        let sys_pid = self.task_data.sys_pid;
        let mut regs: UserRegs = Default::default();
        let mut stopped_task = self.as_stopped_task(&mut regs);
        SyscallEmulator::new(&mut stopped_task).dispatch().await;
        SyscallInfo::orig_nr_to_regs(abi::SYSCALL_BLOCKED, &mut stopped_task.regs);
        ptrace::set_regs(sys_pid, &stopped_task.regs);
        self.cont();
    }
}
