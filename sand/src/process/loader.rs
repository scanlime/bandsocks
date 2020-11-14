use crate::{
    abi, binformat,
    binformat::Header,
    process::{remote::Trampoline, task::StoppedTask},
    protocol::{Errno, FromTask, SysFd, ToTask, VPtr, VString},
};
use sc::{nr, syscall};

pub struct Loader<'q, 's, 't> {
    tr: Trampoline<'q, 's, 't>,
    file: SysFd,
    filename: VString,
    argv: VPtr,
    envp: VPtr,
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

    pub async fn open(
        stopped_task: &'t mut StoppedTask<'q, 's>,
        filename: VString,
        argv: VPtr,
        envp: VPtr,
    ) -> Result<Loader<'q, 's, 't>, Errno> {
        let file = ipc_call!(
            stopped_task.task,
            FromTask::FileOpen {
                dir: None,
                path: filename,
                flags: abi::O_RDONLY as i32,
                mode: 0,
            },
            ToTask::FileReply(result),
            result
        )?;
        Ok(Loader {
            tr: Trampoline::new(stopped_task),
            file,
            filename,
            argv,
            envp,
        })
    }

    pub fn read(&self, offset: usize, bytes: &mut [u8]) -> Result<usize, Errno> {
        let result = unsafe {
            syscall!(
                PREAD64,
                self.file.0,
                bytes.as_mut_ptr(),
                bytes.len(),
                offset
            ) as isize
        };
        if result >= 0 {
            Ok(result as usize)
        } else {
            Err(Errno(result as i32))
        }
    }

    pub fn read_header(&self) -> Result<Header, Errno> {
        let mut bytes = [0u8; abi::BINPRM_BUF_SIZE];
        self.read(0, &mut bytes)?;
        Ok(Header { bytes })
    }

    pub async fn exec(self) -> Result<(), Errno> {
        let header = self.read_header()?;
        binformat::exec(self, header).await
    }

    pub async fn unmap_all_userspace_mem(&mut self) {
        self.tr.unmap_all_userspace_mem().await
    }

    pub async fn mmap(
        &mut self,
        addr: VPtr,
        length: usize,
        prot: isize,
        flags: isize,
        offset: isize,
    ) {
        self.tr.mmap(addr, length, prot, flags, fd, offset).await
    }
}
