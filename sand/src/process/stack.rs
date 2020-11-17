use crate::{
    abi, nolibc,
    process::remote::{mem_write_padded_bytes, RemoteFd, Scratchpad, Trampoline},
    protocol::{Errno, VPtr},
};

const BUILDER_SIZE_LIMIT: usize = 8 * 1024 * 1024;
const INITIAL_STACK_FREE: usize = 128 * 1024;

#[derive(Debug)]
pub struct CantDropStackBuilder;

impl Drop for CantDropStackBuilder {
    fn drop(&mut self) {
        panic!("leaking stackbuilder")
    }
}

#[derive(Debug)]
pub struct StackBuilder {
    memfd: RemoteFd,
    top: VPtr,
    bottom: VPtr,
    lower_limit: VPtr,
    guard: CantDropStackBuilder,
}

fn randomize_stack_top(limit: VPtr) -> VPtr {
    let random_offset = nolibc::getrandom_usize() & abi::STACK_RND_MASK;
    VPtr(abi::page_round_down(
        limit.0 - (random_offset << abi::PAGE_SHIFT),
    ))
}

impl StackBuilder {
    pub async fn new(scratchpad: &mut Scratchpad<'_, '_, '_, '_>) -> Result<Self, Errno> {
        let memfd = scratchpad.memfd_create(b"bandsocks-loader", 0).await?;
        let top = randomize_stack_top(scratchpad.trampoline.task_end);
        let bottom = top;
        let lower_limit = VPtr(top.0 - BUILDER_SIZE_LIMIT);
        Ok(StackBuilder {
            memfd,
            top,
            bottom,
            lower_limit,
            guard: CantDropStackBuilder,
        })
    }

    pub async fn finish(self, trampoline: &mut Trampoline<'_, '_, '_>) -> Result<(), Errno> {
        let stack_size = self.top.0 - self.bottom.0;
        let stack_mapping_base = VPtr(abi::page_round_down(self.bottom.0 - INITIAL_STACK_FREE));
        let stack_mapping_size = abi::page_round_up(self.top.0 - stack_mapping_base.0);

        let prot = abi::PROT_READ | abi::PROT_WRITE;
        let flags = abi::MAP_PRIVATE | abi::MAP_ANONYMOUS | abi::MAP_FIXED | abi::MAP_GROWSDOWN;
        trampoline
            .mmap(
                stack_mapping_base,
                stack_mapping_size,
                prot,
                flags,
                &RemoteFd(0),
                0,
            )
            .await?;

        match trampoline
            .pread(
                &self.memfd,
                self.bottom,
                stack_size,
                self.bottom.0 - self.lower_limit.0,
            )
            .await
        {
            Err(e) => Err(e),
            Ok(actual) if actual as usize == stack_size => Ok(()),
            _ => Err(Errno(-abi::EFAULT as i32)),
        }?;
        trampoline.close(&self.memfd).await?;
        core::mem::forget(self.guard);
        Ok(())
    }

    pub fn stack_top(&self) -> VPtr {
        self.top
    }

    pub fn stack_bottom(&self) -> VPtr {
        self.bottom
    }

    pub fn align(&mut self, alignment: usize) -> VPtr {
        let mask = alignment - 1;
        assert_eq!(alignment & mask, 0);
        let ptr = VPtr(self.bottom.0 & !mask);
        self.bottom = ptr;
        ptr
    }

    pub async fn push_remote_bytes(
        &mut self,
        trampoline: &mut Trampoline<'_, '_, '_>,
        addr: VPtr,
        length: usize,
    ) -> Result<VPtr, Errno> {
        let ptr = VPtr(self.bottom.0 - length);
        if ptr.0 < self.lower_limit.0 {
            return Err(Errno(-abi::E2BIG as i32));
        }
        let file_offset = ptr.0 - self.lower_limit.0;
        match trampoline
            .pwrite(&self.memfd, addr, length, file_offset)
            .await
        {
            Err(e) => Err(e),
            Ok(actual) if actual as usize == length => Ok(()),
            _ => Err(Errno(-abi::EFAULT as i32)),
        }?;
        self.bottom = ptr;
        Ok(ptr)
    }

    pub async fn push_bytes(
        &mut self,
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        bytes: &[u8],
    ) -> Result<VPtr, Errno> {
        mem_write_padded_bytes(
            scratchpad.trampoline.stopped_task,
            scratchpad.page_ptr,
            bytes,
        )
        .map_err(|_| Errno(-abi::EFAULT as i32))?;
        self.push_remote_bytes(scratchpad.trampoline, scratchpad.page_ptr, bytes.len())
            .await
    }
}
