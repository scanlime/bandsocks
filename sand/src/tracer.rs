use crate::{
    abi,
    ipc::Socket,
    process::{table::ProcessTable, Event, SigInfo, TaskFn},
    protocol::SysPid,
    ptrace,
};
use core::{future::Future, pin::Pin};
use pin_project::pin_project;
use sc::syscall;

#[pin_project]
pub struct Tracer<'t, F: Future<Output = ()>> {
    ipc: Socket,
    #[pin]
    process_table: ProcessTable<'t, F>,
}

impl<'p, 't: 'p, F: Future<Output = ()>> Tracer<'t, F> {
    pub fn new(ipc: Socket, task_fn: TaskFn<'t, F>) -> Self {
        Tracer {
            ipc,
            process_table: ProcessTable::new(task_fn),
        }
    }

    pub fn run(&mut self, cmd: &[u8], argv: &[*const u8], envp: &[*const u8]) {
        let mut pin = unsafe { Pin::new_unchecked(self) };
        pin.as_mut().spawn(cmd, argv, envp);
        pin.as_mut().handle_events();
    }

    fn spawn(self: Pin<&'p mut Self>, cmd: &[u8], argv: &[*const u8], envp: &[*const u8]) {
        unsafe {
            match syscall!(FORK) as isize {
                result if result == 0 => ptrace::be_the_child_process(cmd, argv, envp),
                result if result < 0 => panic!("fork error"),
                result => self.expect_new_child(SysPid(result as u32)),
            }
        }
    }

    fn expect_new_child(self: Pin<&'p mut Self>, sys_pid: SysPid) {
        self.project()
            .process_table
            .insert(sys_pid)
            .expect("virtual process limit exceeded");
    }

    fn handle_events(mut self: Pin<&'p mut Self>) {
        let mut siginfo: abi::SigInfo = Default::default();
        loop {
            match ptrace::wait(&mut siginfo) {
                err if err == abi::ECHILD => {
                    // All child processes have exited
                    break;
                }
                err if err == abi::EAGAIN => {
                    // Interrupted by I/O, no event
                }
                err if err == 0 => {
                    let sys_pid = SysPid(siginfo.si_pid);
                    let event = Event::Signal(SigInfo {
                        si_signo: siginfo.si_signo,
                        si_code: siginfo.si_code,
                    });
                    let vpid = self
                        .as_mut()
                        .project()
                        .process_table
                        .as_ref()
                        .syspid_to_v(sys_pid);
                    match vpid {
                        None => panic!("signal for unrecognized {:?}", sys_pid),
                        Some(vpid) => {
                            let process = self.as_mut().project().process_table.get(vpid).unwrap();
                            process.enqueue(event).unwrap();
                            //process.poll();
                        }
                    }
                }
                err => {
                    panic!("unexpected waitid response ({})", err);
                }
            }

            let ipc = &mut self.as_mut().project().ipc;
            while let Some(message) = ipc.recv() {
                println!("received: {:?}", message);
            }
        }
    }
}
