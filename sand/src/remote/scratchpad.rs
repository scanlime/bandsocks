use crate::{
    abi,
    mem::{
        maps::{MappedPages, MemFlags},
        page::VPage,
    },
    protocol::{Errno, VPtr},
    remote::{file::RemoteFd, trampoline::Trampoline},
};
use core::ops::Range;

impl<'q, 's, 't, 'r> Drop for Scratchpad<'q, 's, 't, 'r> {
    fn drop(&mut self) {
        panic!("leaking scratchpad")
    }
}

#[derive(Debug)]
pub struct Scratchpad<'q, 's, 't, 'r> {
    pub trampoline: &'r mut Trampoline<'q, 's, 't>,
    pub mem_range: Range<VPage>,
}

impl<'q, 's, 't, 'r> Scratchpad<'q, 's, 't, 'r> {
    pub async fn new(
        trampoline: &'r mut Trampoline<'q, 's, 't>,
    ) -> Result<Scratchpad<'q, 's, 't, 'r>, Errno> {
        const PAGE_COUNT: usize = 1;
        let mem_range = trampoline
            .mmap(
                &MappedPages::anonymous(VPage::null()..(VPage::null() + PAGE_COUNT)),
                &RemoteFd::invalid(),
                &MemFlags::rw(),
                abi::MAP_ANONYMOUS,
            )
            .await?;
        Ok(Scratchpad {
            trampoline,
            mem_range,
        })
    }

    pub fn len(&self) -> usize {
        self.mem_range.end.ptr().0 - self.mem_range.start.ptr().0
    }

    pub fn ptr(&self) -> VPtr {
        self.mem_range.start.ptr()
    }

    pub async fn free(self) -> Result<(), Errno> {
        self.trampoline.munmap(&self.mem_range).await?;
        core::mem::forget(self);
        Ok(())
    }
}
