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
            let mut unmap = None;
            for map in MapsIterator::new(tr.stopped_task) {
                if map != tr.vdso && map != tr.vvar {
                    let addr = map.start;
                    let length = map.end - map.start + 1;
                    unmap = Some([addr as isize, length as isize]);
                    break;
                }
            }
            match unmap {
                Some(args) => assert_eq!(0, tr.syscall(nr::MUNMAP, &args).await),
                None => break,
            }
        }

        let scratch_ptr = VPtr(0x10000);
        assert_eq!(
            scratch_ptr.0 as isize,
            tr.syscall(
                nr::MMAP,
                &[
                    scratch_ptr.0 as isize,
                    0x100000,
                    abi::PROT_READ | abi::PROT_WRITE,
                    abi::MAP_ANONYMOUS | abi::MAP_PRIVATE | abi::MAP_FIXED
                ]
            )
            .await
        );

        loop {
            let m = b"Hello World!\n";
            remote::mem_write(self.stopped_task, scratch_ptr, m).unwrap();
            assert_eq!(
                m.len() as isize,
                remote::Trampoline::new(self.stopped_task)
                    .syscall(nr::WRITE, &[1, scratch_ptr.0 as isize, m.len() as isize])
                    .await
            );
        }
    }
}
