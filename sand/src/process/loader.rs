use crate::{
    abi,
    process::{maps::MapsIterator, remote, task::StoppedTask},
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

    pub async fn do_exec(self) -> Result<(), Errno> {
        let mut tr = remote::Trampoline::new(self.stopped_task);

        loop {
            let mut to_unmap = None;
            for area in MapsIterator::new(tr.stopped_task) {
                if area != tr.vdso && area != tr.vvar {
                    to_unmap = Some(area);
                    break;
                }
            }
            match to_unmap {
                Some(area) => tr.munmap(area.vptr(), area.len()).await.unwrap(),
                None => break,
            }
        }

        let scratch_ptr = VPtr(0x10000);
        tr.mmap(
            scratch_ptr,
            0x100000,
            abi::PROT_READ | abi::PROT_WRITE,
            abi::MAP_ANONYMOUS | abi::MAP_PRIVATE | abi::MAP_FIXED,
            0,
            0,
        )
        .await
        .unwrap();

        loop {
            let m = b"Hello World!\n";
            remote::mem_write(tr.stopped_task, scratch_ptr, m).unwrap();
            assert_eq!(
                m.len() as isize,
                tr.syscall(nr::WRITE, &[1, scratch_ptr.0 as isize, m.len() as isize])
                    .await
            );

            remote::mem_write(
                tr.stopped_task,
                scratch_ptr,
                &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            )
            .unwrap();
            tr.syscall(nr::NANOSLEEP, &[scratch_ptr.0 as isize, 0])
                .await;
        }
    }
}
