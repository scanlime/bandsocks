use sc::syscall;
use crate::process::{Event, SigInfo, TaskFn, task::TaskData, table::ProcessTable};
use crate::ipc::Socket;
use crate::abi;
use crate::protocol::SysPid;
use crate::ptrace;
use pin_project::pin_project;
use core::pin::Pin;
use core::future::Future;

#[pin_project]
pub struct Tracer<'a, F: Future<Output=()>> {
    ipc: Socket,
    #[pin] process_table: ProcessTable<'a, F>,
}

impl<'a, F: Future<Output=()>> Tracer<'a, F> {
    pub fn new(ipc: Socket, task_fn: TaskFn<'a, TaskData, F>) -> Self {
        Tracer {
            ipc,
            process_table: ProcessTable::new(task_fn)
        }
    }

    pub fn run(&'a mut self, cmd: &[u8], argv: &[*const u8], envp: &[*const u8]) {
        let pin = unsafe { Pin::new_unchecked(self) };
        pin.as_mut().spawn(cmd, argv, envp);
        pin.as_mut().handle_events();
    }

    fn spawn(self: Pin<&mut Self>, cmd: &[u8], argv: &[*const u8], envp: &[*const u8]) {
        unsafe { match syscall!(FORK) as isize {
            result if result == 0 => ptrace::be_the_child_process(cmd, argv, envp),
            result if result < 0 => panic!("fork error"),
            result => self.expect_new_child(SysPid(result as u32)),
        }}
    }

    fn expect_new_child(self: Pin<&mut Self>, sys_pid: SysPid) {
        self.project().process_table.insert(sys_pid).expect("virtual process limit exceeded");
    }

    fn handle_events(self: Pin<&'a mut Self>) {
        let mut siginfo: abi::SigInfo = Default::default();
        let mut project = self.project();
        let ipc = &mut project.ipc;
        let process_table = project.process_table;
        loop {
            match ptrace::wait(&mut siginfo) {
                err if err == abi::ECHILD => {
                    // All child processes have exited
                    break;
                },
                err if err == abi::EAGAIN => {
                    // Interrupted by I/O, no event
                },
                err if err == 0 => {
                    let sys_pid = SysPid(siginfo.si_pid);
                    let event = Event::Signal(SigInfo {
                        si_signo: siginfo.si_signo,
                        si_code: siginfo.si_code
                    });
                    let vpid = process_table.as_ref().syspid_to_v(sys_pid);
                    match vpid {
                        None => panic!("signal for unrecognized {:?}", sys_pid),
                        Some(vpid) => process_table.get(vpid).unwrap().enqueue(event).unwrap()
                    }
                },
                err => {
                    panic!("unexpected waitid response ({})", err);
                }
            }

            while let Some(message) = ipc.recv() {
                println!("received: {:?}", message);
            }
        }
    }
}
