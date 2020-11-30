use crate::{
    abi,
    abi::UserRegs,
    binformat, nolibc,
    process::{maps::MemArea, stack::StackBuilder, task::StoppedTask},
    protocol::{Errno, FromTask, SysFd, ToTask, VPtr, VString},
    remote::{
        mem::{fault_or, read_string_array, vstring_len},
        scratchpad::Scratchpad,
        trampoline::Trampoline,
    },
};

pub struct Loader<'q, 's, 't> {
    trampoline: Trampoline<'q, 's, 't>,
    file: SysFd,
    filename: VString,
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

        let mut header_bytes = [0u8; abi::BINPRM_BUF_SIZE];
        nolibc::pread(&file, &mut header_bytes, 0)?;
        let file_header = FileHeader {
            bytes: header_bytes,
        };

        let trampoline = Trampoline::new(stopped_task);
        Ok(Loader {
            trampoline,
            file,
            filename,
            file_header,
            argv,
            envp,
        })
    }

    pub fn file_header(&self) -> &FileHeader {
        &self.file_header
    }

    pub fn filename(&self) -> VString {
        self.filename
    }

    pub fn argv_read(&mut self, idx: usize) -> Result<Option<VString>, Errno> {
        fault_or(read_string_array(
            &mut self.trampoline.stopped_task,
            self.argv,
            idx,
        ))
    }

    pub fn envp_read(&mut self, idx: usize) -> Result<Option<VString>, Errno> {
        fault_or(read_string_array(
            &mut self.trampoline.stopped_task,
            self.envp,
            idx,
        ))
    }

    pub fn vstring_len(&mut self, ptr: VString) -> Result<usize, Errno> {
        fault_or(vstring_len(&mut self.trampoline.stopped_task, ptr))
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

    pub fn vdso(&self) -> &MemArea {
        &self.trampoline.kernel_mem.vdso
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
        self.trampoline.mmap_anonymous(addr, length, prot).await
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

    pub async fn stack_random_bytes(
        &mut self,
        stack_builder: &mut StackBuilder,
        length: usize,
    ) -> Result<VPtr, Errno> {
        assert!(length <= abi::PAGE_SIZE);
        let mut scratchpad = Scratchpad::new(&mut self.trampoline).await?;
        let result = stack_builder
            .push_random_bytes(&mut scratchpad, length)
            .await;
        scratchpad.free().await?;
        result
    }

    pub async fn stack_bytes(
        &mut self,
        stack_builder: &mut StackBuilder,
        bytes: &[u8],
    ) -> Result<VPtr, Errno> {
        assert!(bytes.len() <= abi::PAGE_SIZE);
        let mut scratchpad = Scratchpad::new(&mut self.trampoline).await?;
        let result = stack_builder.push_bytes(&mut scratchpad, bytes).await;
        scratchpad.free().await?;
        result
    }

    pub async fn stack_finish(&mut self, stack_builder: StackBuilder) -> Result<(), Errno> {
        stack_builder.finish(&mut self.trampoline).await
    }

    pub async fn stack_stored_vectors(
        &mut self,
        stack_builder: &mut StackBuilder,
    ) -> Result<VPtr, Errno> {
        let mut scratchpad = Scratchpad::new(&mut self.trampoline).await?;
        let result = stack_builder.push_stored_vectors(&mut scratchpad).await;
        scratchpad.free().await?;
        result
    }

    pub async fn store_vectors(
        &mut self,
        stack_builder: &mut StackBuilder,
        vectors: &[usize],
    ) -> Result<(), Errno> {
        let mut scratchpad = Scratchpad::new(&mut self.trampoline).await?;
        let result = stack_builder.store_vectors(&mut scratchpad, vectors).await;
        scratchpad.free().await?;
        result
    }
}
