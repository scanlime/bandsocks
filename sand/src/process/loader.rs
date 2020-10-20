use crate::{
    process::{remote, task::StoppedTask},
    protocol::{Errno, VPtr, VString},
};
use sc::nr;

pub struct Loader<'q, 's, 't> {
    stopped_task: &'t mut StoppedTask<'q, 's>,
}

impl<'q, 's, 't> Loader<'q, 's, 't> {
    pub fn from_entrypoint(stopped_task: &'t mut StoppedTask<'q, 's>) -> Loader<'q, 's, 't> {
        Loader { stopped_task }
    }

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
        println!("made it to exec! with {:x?}", self.stopped_task);

        let saved_regs = self.stopped_task.regs.clone();
        let scratch_ptr = VPtr(((self.stopped_task.regs.sp & !0xFFF) - 0x2000) as usize);

        let m = b"doing a big stdout. multiple strings even. whose stack is this anyway!\n";
        remote::mem_write(&mut self.stopped_task, scratch_ptr, m).unwrap();
        assert_eq!(m.len() as isize, remote::syscall(&mut self.stopped_task, nr::WRITE, &[
            1, scratch_ptr.0 as isize, m.len() as isize
        ]).await);

        let m = b"Hello World!\n";
        remote::mem_write(&mut self.stopped_task, scratch_ptr, m).unwrap();
        assert_eq!(m.len() as isize, remote::syscall(&mut self.stopped_task, nr::WRITE, &[
            1, scratch_ptr.0 as isize, m.len() as isize
        ]).await);

        remote::print_maps(self.stopped_task);
        println!("trying munmap");
        let reply = remote::syscall(&mut self.stopped_task, nr::MUNMAP, &[
            0xffffffffff600000 as usize as isize, 0x1000
        ]).await;
        assert_eq!(reply, 0);
        remote::print_maps(self.stopped_task);

        for n in 0u8..100u8 {
            *self.stopped_task.regs = saved_regs.clone();

            let timespec_ptr = (self.stopped_task.regs.sp & !0xFFF) - 0x1000;
            println!(
                "sleep {}, ptr {:x} sp {:x}",
                n, timespec_ptr, self.stopped_task.regs.sp
            );
            let reply = remote::syscall(
                &mut self.stopped_task,
                nr::NANOSLEEP,
                &[timespec_ptr as isize, 0],
            )
            .await;
            println!("reply: {}", reply);
        }

        Err(Errno(-1))
    }
}
