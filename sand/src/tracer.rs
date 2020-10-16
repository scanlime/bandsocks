use crate::{
    abi,
    ipc::Socket,
    process::{table::ProcessTable, Event, SigInfo, TaskFn},
    protocol::SysPid,
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
        pin.handle_events();
    }

    fn spawn(self: Pin<&mut Self>, args: &RawExecArgs) {
        unsafe {
            match syscall!(FORK) as isize {
                result if result == 0 => ptrace::be_the_child_process(args),
                result if result < 0 => panic!("fork error"),
                result => self.expect_new_child(SysPid(result as u32)),
            }
        }
    }

    fn expect_new_child(self: Pin<&mut Self>, sys_pid: SysPid) {
        self.project()
            .process_table
            .insert(sys_pid)
            .expect("virtual process limit exceeded");
    }

    fn handle_events(mut self: Pin<&mut Self>) {
        let mut siginfo: abi::SigInfo = Default::default();
        loop {
            match ptrace::wait(&mut siginfo) {
                err if err == abi::ECHILD => break,
                err if err == abi::EAGAIN => (),
                err if err == 0 => self.as_mut().handle_siginfo(&siginfo),
                err => panic!("unexpected waitid response ({})", err),
            }

            let ipc = &mut self.as_mut().project().ipc;
            while let Some(message) = ipc.recv() {
                println!("received: {:?}", message);
            }
        }
    }

    fn handle_siginfo(self: Pin<&mut Self>, siginfo: &abi::SigInfo) {
        let sys_pid = SysPid(siginfo.si_pid);
        let event = Event::Signal(SigInfo {
            si_signo: siginfo.si_signo,
            si_code: siginfo.si_code,
        });
        match self.project().process_table.get_sys(sys_pid) {
            None => panic!("signal for unrecognized {:?}", sys_pid),
            Some(mut process) => {
                process.as_mut().enqueue(event).unwrap();
                assert_eq!(process.poll(), Poll::Pending);
            }
        }
    }
}
