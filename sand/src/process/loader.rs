use crate::{
    process::{remote, task::StoppedTask},
    protocol::{Errno, VPtr, VString},
};
use sc::nr;

pub struct Loader<'q, 's, 't> {
    stopped_task: &'t mut StoppedTask<'q, 's>,
}

impl<'q, 's, 't> Loader<'q, 's, 't> {
    pub fn from_execve(
        stopped_task: &'t mut StoppedTask<'q, 's>,
        filename: VString,
        argv: VPtr,
        envp: VPtr,
    ) -> Loader<'q, 's, 't> {
        println!("ignoring exec args, {:?} {:?} {:?}", filename, argv, envp);
        Loader { stopped_task }
    }

    pub async fn do_exec(mut self) -> Result<(), Errno> {
        // temp: testing out remote syscalls and memory access so we can build a loader
        // here.
        println!("made it to exec! with {:x?}", self.stopped_task);

        println!(
            "remote pid, {}",
            remote::syscall(&mut self.stopped_task, nr::GETPID, &[]).await
        );

        for n in 0u8..100u8 {
            let timespec_ptr = (self.stopped_task.regs.sp & !0xFFF) - 0x1000;
            println!(
                "sleep {}, ptr {:x} sp {:x}",
                n, timespec_ptr, self.stopped_task.regs.sp
            );
            remote::syscall(
                &mut self.stopped_task,
                nr::NANOSLEEP,
                &[timespec_ptr as isize, 0],
            )
            .await;
        }

        Err(Errno(-1))
    }
}
