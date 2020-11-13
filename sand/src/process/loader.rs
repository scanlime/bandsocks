use crate::{
    abi,
    process::{remote, task::StoppedTask},
    protocol::{Errno, FileBacking, FromTask, ToTask, VPtr, VString},
};
use goblin::elf64;
use sc::{nr, syscall};

pub struct Loader<'q, 's, 't> {
    stopped_task: &'t mut StoppedTask<'q, 's>,
    filename: VString,
    argv: VPtr,
    envp: VPtr,
}

impl<'q, 's, 't> Loader<'q, 's, 't> {
    pub fn new(
        stopped_task: &'t mut StoppedTask<'q, 's>,
        filename: VString,
        argv: VPtr,
        envp: VPtr,
    ) -> Loader<'q, 's, 't> {
        Loader {
            stopped_task,
            filename,
            argv,
            envp,
        }
    }

    pub async fn do_exec(self) -> Result<(), Errno> {
        let result = ipc_call!(
            self.stopped_task.task,
            FromTask::FileOpen {
                dir: None,
                path: self.filename,
                mode: 0,
                flags: abi::O_RDONLY as i32,
            },
            ToTask::FileReply(result),
            result
        );

        println!(
            "result={:?}, argv={:x?} envp={:x?}",
            result, self.argv, self.envp
        );

        if let Ok(FileBacking::VFSMapRef {
            source,
            offset,
            filesize,
        }) = result
        {
            let mut header: elf64::header::Header = Default::default();
            let result = unsafe {
                syscall!(
                    PREAD64,
                    source.0,
                    &mut header as *mut elf64::header::Header,
                    elf64::header::SIZEOF_EHDR,
                    offset
                ) as isize
            };
            println!(
                "read: {:?}, filesize={:?} header={:?}",
                result, filesize, header
            );
        }

        let mut tr = remote::Trampoline::new(self.stopped_task);
        tr.unmap_all_userspace_mem().await;

        let scratch_ptr = VPtr(0x10000);
        tr.mmap(
            scratch_ptr,
            0x1000,
            abi::PROT_READ | abi::PROT_WRITE,
            abi::MAP_ANONYMOUS | abi::MAP_PRIVATE | abi::MAP_FIXED,
            0,
            0,
        )
        .await
        .unwrap();

        loop {
            let m = b"Hello World!\n";
            remote::mem_write_padded_bytes(tr.stopped_task, scratch_ptr, m).unwrap();
            assert_eq!(
                m.len() as isize,
                tr.syscall(nr::WRITE, &[1, scratch_ptr.0 as isize, m.len() as isize])
                    .await
            );

            remote::mem_write_words(tr.stopped_task, scratch_ptr, &[1, 500000000]).unwrap();
            tr.syscall(nr::NANOSLEEP, &[scratch_ptr.0 as isize, 0])
                .await;
        }
    }
}
