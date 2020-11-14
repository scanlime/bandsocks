use crate::{
    abi,
    binformat::header::Header,
    process::{remote, task::StoppedTask},
    protocol::{Errno, FromTask, SysFd, ToTask, VPtr, VString},
};
use sc::nr;

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

    pub async fn exec(self) -> Result<(), Errno> {
        let sys_fd = ipc_call!(
            self.stopped_task.task,
            FromTask::FileOpen {
                dir: None,
                path: self.filename,
                mode: 0,
                flags: abi::O_RDONLY as i32,
            },
            ToTask::FileReply(result),
            result
        )?;
        let result = self.exec_with_fd(&sys_fd).await;
        sys_fd.close().unwrap();
        result
    }

    async fn exec_with_fd(self, sys_fd: &SysFd) -> Result<(), Errno> {
        let header = Header::load(sys_fd)?;
        self.exec_with_header(sys_fd, &header).await
    }

    async fn exec_with_header(self, sys_fd: &SysFd, header: &Header) -> Result<(), Errno> {
        println!(
            "fd={:?} header={:?}, argv={:x?} envp={:x?}",
            sys_fd, header, self.argv, self.envp
        );

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
