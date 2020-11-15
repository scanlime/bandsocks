use crate::{
    abi,
    ipc::Socket,
    process::{table::ProcessTable, task::TaskSocketPair, Event, TaskFn},
    protocol::{MessageFromSand, MessageToSand, SysFd, SysPid, VPid},
    ptrace,
    ptrace::RawExecArgs,
};
use core::{future::Future, pin::Pin, ptr::null, task::Poll};
use heapless::{consts::*, String};
use pin_project::pin_project;
use sc::syscall;

#[pin_project]
pub struct Tracer<'t, F: Future<Output = ()>> {
    ipc: Socket,
    #[pin]
    process_table: ProcessTable<'t, F>,
}

impl<'t, F: Future<Output = ()>> Tracer<'t, F> {
    pub fn new(ipc: Socket, task_fn: TaskFn<'t, F>) -> Self {
        Tracer {
            process_table: ProcessTable::new(task_fn),
            ipc,
        }
    }

    pub fn run(mut self) {
        let pin = unsafe { Pin::new_unchecked(&mut self) };
        pin.event_loop();
    }

    fn init_loader(self: Pin<&mut Self>, args_fd: &SysFd) {
        // The stage 2 loader turns an args socket from the runtime into a normal
        // execve() call
        let mut fd_str = String::<U16>::from("FD=");
        fd_str.push_str(&String::<U16>::from(args_fd.0)).unwrap();
        fd_str.push('\0').unwrap();
        let loader_argv = [crate::STAGE_2_INIT_LOADER.as_ptr(), null()];
        let loader_env = [fd_str.as_ptr(), null()];
        let exec_args = unsafe { RawExecArgs::new(crate::SELF_EXE, &loader_argv, &loader_env) };

        // Each process needs a TaskSocketPair; the first one is created here and passed
        // via fork
        let socket_pair = TaskSocketPair::new();

        match unsafe { syscall!(FORK) } as isize {
            result if result == 0 => unsafe { ptrace::be_the_child_process(&exec_args) },
            result if result < 0 => panic!("fork error"),
            result => {
                let sys_pid = SysPid(result as u32);
                let parent = None;
                self.project()
                    .process_table
                    .insert(sys_pid, parent, socket_pair)
                    .expect("virtual process limit exceeded");
            }
        }
    }

    fn event_loop(mut self: Pin<&mut Self>) {
        let mut siginfo: abi::SigInfo = Default::default();
        loop {
            while let Some(message) = self.as_mut().project().ipc.recv() {
                self.as_mut().message_event(message);
            }
            match ptrace::wait(&mut siginfo) {
                err if err == -abi::ECHILD => break,
                err if err == -abi::EINTR => (),
                err if err == 0 => self.as_mut().siginfo_event(&siginfo),
                err => panic!("unexpected waitid response ({})", err),
            }
        }
    }

    fn message_event(self: Pin<&mut Self>, message: MessageToSand) {
        match message {
            MessageToSand::Task { task, op } => self.task_event(task, Event::Message(op)),
            MessageToSand::Init { args } => self.init_loader(&args),
        }
    }

    fn siginfo_event(mut self: Pin<&mut Self>, siginfo: &abi::SigInfo) {
        let sys_pid = SysPid(siginfo.si_pid);
        let vpid = self.as_mut().project().process_table.syspid_to_v(sys_pid);
        match vpid {
            None => panic!("signal for unrecognized task, {:x?}", sys_pid),
            Some(vpid) => {
                self.task_event(
                    vpid,
                    Event::Signal {
                        sig: siginfo.si_signo,
                        code: siginfo.si_code,
                        status: siginfo.si_status,
                    },
                );
            }
        }
    }

    fn task_event(mut self: Pin<&mut Self>, task: VPid, event: Event) {
        let process = self.as_mut().project().process_table.get(task);
        match process {
            None => panic!("message for unrecognized task, {:x?}", task),
            Some(mut process) => {
                let result = process.as_mut().send_event(event);
                result.expect("event queue full");
                assert_eq!(process.as_mut().poll(), Poll::Pending);
            }
        }
        loop {
            let process = self.as_mut().project().process_table.get(task);
            let outbox = process.unwrap().as_mut().check_outbox();
            match outbox {
                None => break,
                Some(op) => {
                    let ipc = self.as_mut().project().ipc;
                    ipc.send(&MessageFromSand::Task { task, op });
                }
            }
        }
    }
}
