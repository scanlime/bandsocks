use crate::{
    abi,
    abi::UserRegs,
    binformat, nolibc,
    process::{stack::StackBuilder, task::StoppedTask},
    protocol::{Errno, FromTask, SysFd, ToTask, VPtr, VString},
    remote::{mem::read_string_array, mem::fault_or, scratchpad::Scratchpad, trampoline::Trampoline, RemoteFd},
};

pub struct Loader<'q, 's, 't> {
    trampoline: Trampoline<'q, 's, 't>,
    file: SysFd,
    file_header: FileHeader,
    argv: VPtr,
    envp: VPtr,
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
        nolibc::pread(&file, &mut header_bytes, 0)?;
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

    pub fn argv_read(&mut self, idx: usize) -> Result<Option<VString>, Errno> {
        fault_or(read_string_array(&mut self.trampoline.stopped_task, self.argv, idx))
    }

    pub fn envp_read(&mut self, idx: usize) -> Result<Option<VString>, Errno> {
        fault_or(read_string_array(&mut self.trampoline.stopped_task, self.envp, idx))
    }

    pub fn read_file(&self, offset: usize, bytes: &mut [u8]) -> Result<usize, Errno> {
        nolibc::pread(&self.file, bytes, offset)
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

    pub fn randomize_brk(&mut self, brk_base: VPtr) {
        let random_offset = nolibc::getrandom_usize() & abi::BRK_RND_MASK;
        let brk = VPtr(abi::page_round_up(
            brk_base.0 + (random_offset << abi::PAGE_SHIFT),
        ));
        let mut mm = &mut self.trampoline.stopped_task.task.task_data.mm;
        mm.brk = brk;
        mm.brk_start = brk;
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
