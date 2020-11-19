use crate::{
    abi, nolibc,
    protocol::{Errno, VPtr},
    remote::{
        mem::{fault_or, write_padded_bytes, write_word},
        scratchpad::{Scratchpad, TempRemoteFd},
        trampoline::Trampoline,
        RemoteFd,
    },
};
use core::mem::size_of;

const BUILDER_SIZE_LIMIT: usize = 32 * 1024 * 1024;
const INITIAL_STACK_FREE: usize = 128 * 1024;

#[derive(Debug)]
pub struct StackBuilder {
    memfd: TempRemoteFd,
    top: VPtr,
    bottom: VPtr,
    num_stored_vectors: usize,
}

fn randomize_stack_top(limit: VPtr) -> VPtr {
    let random_offset = nolibc::getrandom_usize() & abi::STACK_RND_MASK;
    VPtr(abi::page_round_down(
        limit.0 - (random_offset << abi::PAGE_SHIFT),
    ))
}

impl StackBuilder {
    pub async fn new(scratchpad: &mut Scratchpad<'_, '_, '_, '_>) -> Result<Self, Errno> {
        // this is a tmpfs sparse file that holds the stack we're building, in two
        // sections: growing downward from BUILDER_SIZE_LIMIT is the stack
        // itself, and growing up from there is a temporary location to store
        // vectors that will go to the bottom of the stack later.
        let top = randomize_stack_top(scratchpad.trampoline.task_end);
        Ok(StackBuilder {
            memfd: scratchpad.memfd_temp().await?,
            top,
            bottom: top,
            num_stored_vectors: 0,
        })
    }

    pub async fn finish(self, trampoline: &mut Trampoline<'_, '_, '_>) -> Result<(), Errno> {
        assert_eq!(self.num_stored_vectors, 0);
        let stack_size = self.top.0 - self.bottom.0;
        let stack_mapping_base = VPtr(abi::page_round_down(self.bottom.0 - INITIAL_STACK_FREE));
        let stack_mapping_size = abi::page_round_up(self.top.0 - stack_mapping_base.0);
        let file_offset = BUILDER_SIZE_LIMIT - stack_size;
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
        trampoline
            .pread_exact(&self.memfd.0, self.bottom, stack_size, file_offset)
            .await?;
        self.memfd.free(trampoline).await?;
        Ok(())
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
        let stack_size = self.top.0 - ptr.0;
        if stack_size > BUILDER_SIZE_LIMIT {
            return Err(Errno(-abi::E2BIG));
        }
        let file_offset = BUILDER_SIZE_LIMIT - stack_size;
        trampoline
            .pwrite_exact(&self.memfd.0, addr, length, file_offset)
            .await?;
        self.bottom = ptr;
        Ok(ptr)
    }

    pub async fn push_random_bytes(
        &mut self,
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        length: usize,
    ) -> Result<VPtr, Errno> {
        assert!(length < abi::PAGE_SIZE);
        scratchpad
            .trampoline
            .getrandom_exact(scratchpad.page_ptr, length, 0)
            .await?;
        self.push_remote_bytes(scratchpad.trampoline, scratchpad.page_ptr, length)
            .await
    }

    pub async fn push_bytes(
        &mut self,
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        bytes: &[u8],
    ) -> Result<VPtr, Errno> {
        assert!(bytes.len() < abi::PAGE_SIZE);
        fault_or(write_padded_bytes(
            scratchpad.trampoline.stopped_task,
            scratchpad.page_ptr,
            bytes,
        ))?;
        self.push_remote_bytes(scratchpad.trampoline, scratchpad.page_ptr, bytes.len())
            .await
    }

    pub async fn push_stored_vectors(
        &mut self,
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
    ) -> Result<VPtr, Errno> {
        let length = self.num_stored_vectors * size_of::<usize>();
        let ptr = VPtr(self.bottom.0 - length);
        let stack_size = self.top.0 - ptr.0;
        if stack_size > BUILDER_SIZE_LIMIT {
            return Err(Errno(-abi::E2BIG));
        }
        let file_offset = BUILDER_SIZE_LIMIT - stack_size;
        scratchpad
            .fd_copy_exact(
                &self.memfd.0,
                BUILDER_SIZE_LIMIT,
                &self.memfd.0,
                file_offset,
                length,
            )
            .await?;
        self.bottom = ptr;
        self.num_stored_vectors = 0;
        Ok(ptr)
    }

    /// store a small number of usize vectors, for later adding via
    /// push_stored_vectors(). this individual buffer must fit in a page,
    /// but there is no limit (other than tmpfs size) for the total size
    /// of vectors we store before pushing.
    pub async fn store_vectors(
        &mut self,
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        vectors: &[usize],
    ) -> Result<(), Errno> {
        let length = vectors.len() * size_of::<usize>();
        assert!(length <= abi::PAGE_SIZE);
        for (i, word) in vectors.iter().enumerate() {
            fault_or(write_word(
                scratchpad.trampoline.stopped_task,
                scratchpad.page_ptr.add(i * size_of::<usize>()),
                *word,
            ))?;
        }
        let file_offset = BUILDER_SIZE_LIMIT + self.num_stored_vectors * size_of::<usize>();
        scratchpad
            .trampoline
            .pwrite_exact(&self.memfd.0, scratchpad.page_ptr, length, file_offset)
            .await?;
        self.num_stored_vectors += vectors.len();
        Ok(())
    }
}
