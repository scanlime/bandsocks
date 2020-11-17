use crate::{
    abi,
    abi::UserRegs,
    binformat,
    nolibc::pread,
    process::{
        remote::{RemoteFd, Scratchpad, Trampoline},
        stack::StackBuilder,
        task::StoppedTask,
    },
    protocol::{Errno, FromTask, SysFd, ToTask, VPtr, VString},
};
use sc::nr;

pub struct Loader<'q, 's, 't> {
    trampoline: Trampoline<'q, 's, 't>,
    file: SysFd,
    file_header: FileHeader,
    pub argv: VPtr,
    pub envp: VPtr,
}

#[derive(Debug)]
#[repr(C)]
#[repr(align(8))]
pub struct FileHeader {
    pub bytes: [u8; abi::BINPRM_BUF_SIZE],
}

#[derive(Debug, Clone)]
pub struct MemLayout {
    pub start_code: VPtr,
    pub end_code: VPtr,
    pub start_data: VPtr,
    pub end_data: VPtr,
    pub start_stack: VPtr,
    pub start_brk: VPtr,
    pub brk: VPtr,
    pub arg_start: VPtr,
    pub arg_end: VPtr,
    pub env_start: VPtr,
    pub env_end: VPtr,
    pub auxv_ptr: VPtr,
    pub auxv_len: usize,
    pub exe_file: VPtr,
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
        argv: VPtr,
        envp: VPtr,
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
        pread(&file, &mut header_bytes, 0)?;
        let file_header = FileHeader {
            bytes: header_bytes,
        };

        let trampoline = Trampoline::new(stopped_task);
        Ok(Loader {
            trampoline,
            file,
            file_header,
            argv,
            envp,
        })
    }

    pub fn read(&self, offset: usize, bytes: &mut [u8]) -> Result<usize, Errno> {
        pread(&self.file, bytes, offset)
    }

    pub async fn exec(self) -> Result<(), Errno> {
        binformat::exec(self).await
    }

    pub async fn unmap_all_userspace_mem(&mut self) {
        self.trampoline.unmap_all_userspace_mem().await;
    }

    pub fn userspace_regs(&mut self) -> &mut UserRegs {
        &mut self.trampoline.stopped_task.regs
    }

    /// Tell the kernel about a new process memory layout. Memory regions must
    /// already exist and have the correct protection flags.
    pub async fn set_mem_layout(&mut self, ml: &MemLayout) -> Result<(), Errno> {
        for args in &[
            [abi::PR_SET_MM_START_CODE, ml.start_code.0 as isize, 0],
            [abi::PR_SET_MM_END_CODE, ml.end_code.0 as isize, 0],
            [abi::PR_SET_MM_START_DATA, ml.start_data.0 as isize, 0],
            [abi::PR_SET_MM_END_DATA, ml.end_data.0 as isize, 0],
            [abi::PR_SET_MM_START_STACK, ml.start_stack.0 as isize, 0],
            [abi::PR_SET_MM_START_BRK, ml.start_brk.0 as isize, 0],
            [abi::PR_SET_MM_BRK, ml.brk.0 as isize, 0],
            [abi::PR_SET_MM_ARG_START, ml.arg_start.0 as isize, 0],
            [abi::PR_SET_MM_ARG_END, ml.arg_end.0 as isize, 0],
            [abi::PR_SET_MM_ENV_START, ml.env_start.0 as isize, 0],
            [abi::PR_SET_MM_ENV_END, ml.env_end.0 as isize, 0],
            [abi::PR_SET_MM_EXE_FILE, ml.exe_file.0 as isize, 0],
            [
                abi::PR_SET_MM_AUXV,
                ml.auxv_ptr.0 as isize,
                ml.auxv_len as isize,
            ],
        ] {
            let result = self.trampoline.syscall(nr::PRCTL, args).await;
            if result != 0 {
                return Err(Errno(result as i32));
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
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

    pub async fn map_file(
        &mut self,
        addr: VPtr,
        length: usize,
        offset: usize,
        prot: isize,
    ) -> Result<VPtr, Errno> {
        let flags = abi::MAP_PRIVATE | abi::MAP_FIXED;
        let mut scratchpad = Scratchpad::new(&mut self.trampoline).await?;
        let sent_fd = scratchpad.send_fd(&self.file).await;
        scratchpad.free().await?;
        let sent_fd = sent_fd?;
        let result = self
            .trampoline
            .mmap(addr, length, prot, flags, &sent_fd, offset)
            .await;
        self.trampoline.close(&sent_fd).await?;
        result
    }

    pub async fn map_anonymous(
        &mut self,
        addr: VPtr,
        length: usize,
        prot: isize,
    ) -> Result<VPtr, Errno> {
        let flags = abi::MAP_PRIVATE | abi::MAP_ANONYMOUS | abi::MAP_FIXED;
        self.trampoline
            .mmap(addr, length, prot, flags, &RemoteFd(0), 0)
            .await
    }

    pub async fn stack_begin(&mut self) -> Result<StackBuilder, Errno> {
        let mut scratchpad = Scratchpad::new(&mut self.trampoline).await?;
        let result = StackBuilder::new(&mut scratchpad).await;
        scratchpad.free().await?;
        result
    }

    pub async fn stack_remote_bytes(
        &mut self,
        stack_builder: &mut StackBuilder,
        addr: VPtr,
        length: usize,
    ) -> Result<VPtr, Errno> {
        stack_builder
            .push_remote_bytes(&mut self.trampoline, addr, length)
            .await
    }

    pub async fn stack_bytes(
        &mut self,
        stack_builder: &mut StackBuilder,
        bytes: &[u8],
    ) -> Result<VPtr, Errno> {
        let mut scratchpad = Scratchpad::new(&mut self.trampoline).await?;
        let result = stack_builder.push_bytes(&mut scratchpad, bytes).await;
        scratchpad.free().await?;
        result
    }

    pub async fn stack_finish(&mut self, stack_builder: StackBuilder) -> Result<(), Errno> {
        stack_builder.finish(&mut self.trampoline).await
    }
}
