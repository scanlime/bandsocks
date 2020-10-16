use crate::{
    abi,
    ipc::Socket,
    process::{table::ProcessTable, Event, TaskFn},
    protocol::{MessageFromSand, MessageToSand, SysPid, VPid},
    ptrace,
    ptrace::RawExecArgs,
};
use core::{future::Future, pin::Pin, task::Poll};
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

    pub fn run(mut self, args: &RawExecArgs) {
        let mut pin = unsafe { Pin::new_unchecked(&mut self) };
        pin.as_mut().spawn(args);
        pin.event_loop();
    }

    fn spawn(self: Pin<&mut Self>, args: &RawExecArgs) {
        match unsafe { syscall!(FORK) } as isize {
            result if result == 0 => unsafe { ptrace::be_the_child_process(args) },
            result if result < 0 => panic!("fork error"),
            result => self.expect_new_child(SysPid(result as u32)),
        }
    }

    fn expect_new_child(self: Pin<&mut Self>, sys_pid: SysPid) {
        self.project()
            .process_table
            .insert(sys_pid)
            .expect("virtual process limit exceeded");
    }

    fn event_loop(mut self: Pin<&mut Self>) {
        let mut siginfo: abi::SigInfo = Default::default();
        loop {
            println!("event loop");
            match ptrace::wait(&mut siginfo) {
                err if err == abi::ECHILD => break,
                err if err == abi::EAGAIN => (),
                err if err == 0 => self.as_mut().siginfo_event(&siginfo),
                err => panic!("unexpected waitid response ({})", err),
            }
            loop {
                let ipc = &mut self.as_mut().project().ipc;
                let message = ipc.recv();
                match message {
                    None => break,
                    Some(m) => self.as_mut().message_event(m),
                }
            }
        }
    }

    fn message_event(self: Pin<&mut Self>, message: MessageToSand) {
        self.task_event(message.task, Event::Message(message.op));
    }

    fn siginfo_event(mut self: Pin<&mut Self>, siginfo: &abi::SigInfo) {
        let sys_pid = SysPid(siginfo.si_pid);
        let vpid = self.as_mut().project().process_table.syspid_to_v(sys_pid);
        match vpid {
            None => panic!("signal for unrecognized {:?}", sys_pid),
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
            None => panic!("message to unrecognized task {:?}", task),
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
                    ipc.send(&MessageFromSand { task, op });
                }
            }
        }
    }
}
