use crate::{
    abi,
    nolibc::File,
    process::{
        maps::print_maps_dump, syscall::SyscallEmulator, table::FileTable, Event, EventSource,
        MessageSender,
    },
    protocol::{
        abi::{Syscall, UserRegs},
        FromTask, LogLevel, LogMessage, ProcessHandle, SysPid, ToTask, TracerSettings, VPid, VPtr,
    },
    ptrace,
    remote::{file::RemoteFd, mem::print_stack_dump},
};
use core::fmt::{self, Debug, Formatter};

#[derive(Debug)]
pub struct TaskSocketPair {
    pub tracer: File,
    pub remote: RemoteFd,
}

#[derive(Debug, Clone)]
pub struct TaskMemManagement {
    // brk is emulated, since the real kernel's brk_start can't be changed without privileges
    pub brk: VPtr,
    pub brk_start: VPtr,
}

#[derive(Debug)]
pub struct TaskData {
    pub vpid: VPid,
    pub sys_pid: SysPid,
    pub parent: Option<VPid>,
    pub socket_pair: TaskSocketPair,
    pub mm: TaskMemManagement,
    pub file_table: FileTable,
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
            File::socketpair(abi::AF_UNIX, abi::SOCK_STREAM, 0).expect("task socket pair");
        tracer
            .fcntl(abi::F_SETFD, abi::F_CLOEXEC)
            .expect("task socket fcntl");
        remote.fcntl(abi::F_SETFD, 0).expect("task socket fcntl");
        // The file will be inherited
        let remote = RemoteFd(remote.fd.0);
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
        expect_event_or_panic(
            &mut events,
            task_data.sys_pid,
            Event::Signal {
                sig: abi::SIGCHLD as u32,
                code: abi::CLD_TRAPPED,
                status: abi::SIGSTOP as u32,
            },
        )
        .await;

        ptrace::cont(task_data.sys_pid);
        expect_event_or_panic(
            &mut events,
            task_data.sys_pid,
            Event::Signal {
                sig: abi::SIGCHLD as u32,
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
            event => {
                unexpected_event_panic(task_data.sys_pid, None, event, ExpectedEvent::OpenProcess)
                    .await
            }
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
                Event::Signal { sig, code, status }
                    if sig == abi::SIGCHLD as u32
                        && code == abi::CLD_TRAPPED
                        && status == abi::PTRACE_SIG_FORK =>
                {
                    let child_pid = ptrace::geteventmsg(self.task_data.sys_pid) as u32;
                    self.handle_fork(child_pid).await
                }
                Event::Signal { sig, code, status }
                    if sig == abi::SIGCHLD as u32
                        && code == abi::CLD_TRAPPED
                        && status == abi::PTRACE_SIG_SECCOMP =>
                {
                    self.handle_seccomp_trap().await
                }
                Event::Signal { sig, code, status }
                    if sig == abi::SIGCHLD as u32 && code == abi::CLD_TRAPPED && status < 0x100 =>
                {
                    self.handle_signal(status as u8).await
                }
                Event::Signal { sig, code, status }
                    if sig == abi::SIGCHLD as u32 && code == abi::CLD_EXITED =>
                {
                    return self.handle_exited(status).await
                }
                event => {
                    let mut regs: UserRegs = Default::default();
                    let sys_pid = self.task_data.sys_pid;
                    let mut stopped_task = self.as_stopped_task(&mut regs);
                    unexpected_event_panic(
                        sys_pid,
                        Some(&mut stopped_task),
                        event,
                        ExpectedEvent::MainLoop,
                    )
                    .await
                }
            }
        }
    }

    fn cont(&self) {
        if self.task_data.tracer_settings.instruction_trace {
            ptrace::single_step(self.task_data.sys_pid);
        } else {
            ptrace::cont(self.task_data.sys_pid);
        }
    }

    fn as_stopped_task<'s>(&'s mut self, regs: &'s mut UserRegs) -> StoppedTask<'q, 's> {
        ptrace::get_regs(self.task_data.sys_pid, regs);
        StoppedTask { task: self, regs }
    }

    async fn handle_signal(&mut self, signal: u8) {
        let mut regs: UserRegs = Default::default();
        let mut stopped_task = self.as_stopped_task(&mut regs);
        let mut log_level = LogLevel::Trace;

        if signal == abi::SIGSEGV || signal == abi::SIGSYS {
            println!("task state:\n{:x?}", stopped_task.regs);
            print_maps_dump(&mut stopped_task);
            print_stack_dump(&mut stopped_task);
            panic!("*** signal {} inside sandbox ***", signal);
        }

        if signal == abi::SIGTRAP {
            log_level = stopped_task.task.task_data.tracer_settings.max_log_level;
        }

        let msg = LogMessage::Signal(signal, stopped_task.regs.clone());
        self.log(log_level, msg);
        self.cont();
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
        Syscall::orig_nr_to_regs(abi::SYSCALL_BLOCKED, &mut stopped_task.regs);
        ptrace::set_regs(sys_pid, &stopped_task.regs);
        self.cont();
    }
}

async fn expect_event_or_panic<'q, 's, 't>(
    events: &'s mut EventSource<'q>,
    sys_pid: SysPid,
    expected: Event,
) {
    let received = events.next().await;
    if received != expected {
        unexpected_event_panic(sys_pid, None, received, ExpectedEvent::Matching(expected)).await;
    }
}

impl<'q, 's> StoppedTask<'q, 's> {
    pub async fn expect_event_or_panic(&mut self, expected: Event) {
        let sys_pid = self.task.task_data.sys_pid;
        let received = self.task.events.next().await;
        if received != expected {
            unexpected_event_panic(
                sys_pid,
                Some(self),
                received,
                ExpectedEvent::Matching(expected),
            )
            .await;
        }
    }
}

#[derive(Debug)]
enum ExpectedEvent {
    Matching(Event),
    MainLoop,
    OpenProcess,
}

async fn unexpected_event_panic<'q, 's, 't>(
    sys_pid: SysPid,
    stopped_task: Option<&'t mut StoppedTask<'q, 's>>,
    received: Event,
    expected: ExpectedEvent,
) -> ! {
    let mut regs: UserRegs = Default::default();
    ptrace::get_regs(sys_pid, &mut regs);
    println!(
        concat!("task: {:?}\n", "stopped: {:x?}\n", "current: {:x?}",),
        sys_pid, stopped_task, regs
    );
    if let Some(stopped_task) = stopped_task {
        print_maps_dump(stopped_task);
        print_stack_dump(stopped_task);
    }
    panic!(
        "*** unexpected event ***\nexpected?: {:?}\nreceived: {:?}",
        expected, received
    );
}
