use crate::{
    abi, binformat,
    nolibc::pread,
    process::{
        remote::{RemoteFd, Scratchpad, Trampoline},
        task::StoppedTask,
    },
    protocol::{Errno, FromTask, SysFd, ToTask, VPtr, VString},
};

pub struct Loader<'q, 's, 't> {
    trampoline: Trampoline<'q, 's, 't>,
    file: SysFd,
    file_header: FileHeader,
}

#[derive(Debug)]
#[repr(C)]
#[repr(align(8))]
pub struct FileHeader {
    pub bytes: [u8; abi::BINPRM_BUF_SIZE],
}

impl<'q, 's, 't> Drop for Loader<'q, 's, 't> {
    fn drop(&mut self) {
        self.file.close().unwrap();
    }
}

impl<'q, 's, 't> Loader<'q, 's, 't> {
    pub async fn execve(
        stopped_task: &'t mut StoppedTask<'q, 's>,
        filename: VString,
        argv: VPtr,
        envp: VPtr,
    ) -> Result<(), Errno> {
        Loader::open(stopped_task, filename, argv, envp)
            .await?
            .exec()
            .await
    }

    pub fn file_header(&self) -> &FileHeader {
        &self.file_header
    }

    pub async fn open(
        stopped_task: &'t mut StoppedTask<'q, 's>,
        file_name: VString,
        _argv: VPtr,
        _envp: VPtr,
    ) -> Result<Loader<'q, 's, 't>, Errno> {
        let file = ipc_call!(
            stopped_task.task,
            FromTask::FileOpen {
                dir: None,
                path: file_name,
                flags: abi::O_RDONLY as i32,
                mode: 0,
            },
            ToTask::FileReply(result),
            result
        )?;
        let mut header_bytes = [0u8; abi::BINPRM_BUF_SIZE];
        pread(&file, 0, &mut header_bytes)?;
        let file_header = FileHeader {
            bytes: header_bytes,
        };
        let trampoline = Trampoline::new(stopped_task);
        Ok(Loader {
            trampoline,
            file,
            file_header,
        })
    }

    pub fn read(&self, offset: usize, bytes: &mut [u8]) -> Result<usize, Errno> {
        pread(&self.file, offset, bytes)
    }

    pub async fn exec(self) -> Result<(), Errno> {
        binformat::exec(self).await
    }

    pub async fn unmap_all_userspace_mem(&mut self) {
        self.trampoline.unmap_all_userspace_mem().await
    }

    pub async fn debug_loop(&mut self) -> ! {
        let fd = RemoteFd(1);
        let mut scratchpad = Scratchpad::new(&mut self.trampoline).await.unwrap();
        loop {
            scratchpad.write_fd(&fd, b"debug loop\n").await.unwrap();
            scratchpad
                .sleep(&abi::TimeSpec {
                    tv_sec: 10,
                    tv_nsec: 0,
                })
                .await
                .unwrap();
        }
    }

    pub async fn mmap(
        &mut self,
        addr: VPtr,
        length: usize,
        prot: isize,
        flags: isize,
        offset: usize,
    ) -> Result<VPtr, Errno> {
        let mut scratchpad = Scratchpad::new(&mut self.trampoline).await?;
        let result = match scratchpad.send_fd(&self.file).await {
            Err(e) => Err(e),
            Ok(fd) => {
                scratchpad
                    .trampoline
                    .mmap(addr, length, prot, flags, fd, offset)
                    .await
            }
        };
        scratchpad.free().await?;
        result
    }
}
