use crate::{
    abi,
    mem::{
        maps::{MappedPages, MemFlags},
        page::VPage,
        rw::{write_padded_bytes, write_word},
    },
    nolibc,
    protocol::{Errno, VPtr},
    remote::{
        file::{fd_copy_exact, RemoteFd, TempRemoteFd},
        scratchpad::Scratchpad,
        trampoline::Trampoline,
    },
};
use core::{mem::size_of, ops::Range};

const BUILDER_SIZE_LIMIT: usize = 256 * 1024 * 1024;
const INITIAL_STACK_FREE: usize = 64 * 1024;

#[derive(Debug)]
pub struct StackBuilder {
    memfd: TempRemoteFd,
    top: VPage,
    bottom: VPtr,
    num_stored_vectors: usize,
}

#[derive(Debug)]
pub struct InitialStack {
    pub sp: VPtr,
    pub pages: Range<VPage>,
}

impl InitialStack {
    async fn load(
        trampoline: &mut Trampoline<'_, '_, '_>,
        memfd: &TempRemoteFd,
        top: VPage,
        bottom: VPtr,
    ) -> Result<Self, Errno> {
        let stack = InitialStack {
            sp: bottom,
            pages: VPage::round_down(VPtr(bottom.0 - INITIAL_STACK_FREE))..top,
        };

        trampoline
            .mmap_fixed(
                &MappedPages::anonymous(stack.pages.clone()),
                &RemoteFd::invalid(),
                &MemFlags::rw(),
                abi::MAP_ANONYMOUS | abi::MAP_FIXED_NOREPLACE | abi::MAP_GROWSDOWN,
            )
            .await?;

        let stack_size = stack.pages.end.ptr().0 - stack.sp.0;
        let file_offset = BUILDER_SIZE_LIMIT - stack_size;

        memfd
            .0
            .pread_vptr(trampoline, stack.sp, stack_size, file_offset)
            .await?;

        Ok(stack)
    }
}

fn randomize_stack_top(limit: VPage) -> VPage {
    limit - (nolibc::getrandom_usize() & abi::STACK_RND_MASK)
}

impl StackBuilder {
    pub async fn new(scratchpad: &mut Scratchpad<'_, '_, '_, '_>) -> Result<Self, Errno> {
        // this is a tmpfs sparse file that holds the stack we're building, in two
        // sections: growing downward from BUILDER_SIZE_LIMIT is the stack
        // itself, and growing up from there is a temporary location to store
        // vectors that will go to the bottom of the stack later.
        let top = randomize_stack_top(scratchpad.trampoline.kernel_mem.task_end);
        Ok(StackBuilder {
            memfd: TempRemoteFd::new(scratchpad).await?,
            top,
            bottom: top.ptr(),
            num_stored_vectors: 0,
        })
    }

    pub async fn load(
        self,
        trampoline: &mut Trampoline<'_, '_, '_>,
    ) -> Result<InitialStack, Errno> {
        assert_eq!(self.num_stored_vectors, 0);
        let result = InitialStack::load(trampoline, &self.memfd, self.top, self.bottom).await;
        self.memfd.free(trampoline).await?;
        result
    }

    pub fn align(&mut self, alignment: usize) -> VPtr {
        let mask = alignment - 1;
        assert_eq!(alignment & mask, 0);
        let ptr = VPtr(self.bottom.0 & !mask);
        self.bottom = ptr;
        ptr
    }

    pub fn skip_bytes(&mut self, length: usize) -> Result<VPtr, Errno> {
        let ptr = VPtr(self.bottom.0 - length);
        let stack_size = self.top.ptr().0 - ptr.0;
        if stack_size > BUILDER_SIZE_LIMIT {
            return Err(Errno(-abi::E2BIG));
        }
        self.bottom = ptr;
        Ok(ptr)
    }

    pub async fn push_remote_bytes(
        &mut self,
        trampoline: &mut Trampoline<'_, '_, '_>,
        range: Range<VPtr>,
    ) -> Result<VPtr, Errno> {
        let length = range.end.0 - range.start.0;
        let ptr = self.skip_bytes(length)?;
        let stack_size = self.top.ptr().0 - ptr.0;
        let file_offset = BUILDER_SIZE_LIMIT - stack_size;
        self.memfd
            .0
            .pwrite_vptr_exact(trampoline, range.start, length, file_offset)
            .await?;
        Ok(ptr)
    }

    pub async fn push_random_bytes(
        &mut self,
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        length: usize,
    ) -> Result<VPtr, Errno> {
        let mem = &scratchpad.mem_range;
        assert!(mem.start.ptr() + length < mem.end.ptr());
        scratchpad
            .trampoline
            .getrandom_exact(mem.start.ptr(), length, 0)
            .await?;
        self.push_remote_bytes(
            scratchpad.trampoline,
            mem.start.ptr()..(mem.start.ptr() + length),
        )
        .await
    }

    pub async fn push_bytes(
        &mut self,
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        bytes: &[u8],
    ) -> Result<VPtr, Errno> {
        let mem = &scratchpad.mem_range;
        assert!(mem.start.ptr() + bytes.len() < mem.end.ptr());
        write_padded_bytes(scratchpad.trampoline.stopped_task, mem.start.ptr(), bytes)?;
        self.push_remote_bytes(
            scratchpad.trampoline,
            mem.start.ptr()..(mem.start.ptr() + bytes.len()),
        )
        .await
    }

    pub async fn push_stored_vectors(
        &mut self,
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
    ) -> Result<VPtr, Errno> {
        let length = self.num_stored_vectors * size_of::<usize>();
        let ptr = VPtr(self.bottom.0 - length);
        let stack_size = self.top.ptr().0 - ptr.0;
        if stack_size > BUILDER_SIZE_LIMIT {
            return Err(Errno(-abi::E2BIG));
        }
        let file_offset = BUILDER_SIZE_LIMIT - stack_size;
        fd_copy_exact(
            scratchpad,
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

    pub fn stored_vector_count(&self) -> usize {
        self.num_stored_vectors
    }

    pub fn stored_vector_bytes(&self) -> usize {
        self.stored_vector_count() * size_of::<usize>()
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
        assert!(length <= scratchpad.len());
        for (i, word) in vectors.iter().enumerate() {
            write_word(
                scratchpad.trampoline.stopped_task,
                scratchpad.ptr() + (i * size_of::<usize>()),
                *word,
            )?;
        }
        let file_offset = BUILDER_SIZE_LIMIT + self.num_stored_vectors * size_of::<usize>();
        self.memfd
            .0
            .pwrite_vptr_exact(scratchpad.trampoline, scratchpad.ptr(), length, file_offset)
            .await?;
        self.num_stored_vectors += vectors.len();
        Ok(())
    }
}
