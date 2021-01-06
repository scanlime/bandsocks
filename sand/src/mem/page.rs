use crate::{abi, nolibc, protocol::VPtr};
use core::{
    fmt,
    ops::{Add, Range, Sub},
};

pub fn page_offset(value: usize) -> usize {
    value & (abi::PAGE_SIZE - 1)
}

/// A pointer to the beginning of a page in process address space
#[derive(PartialEq, Eq, Ord, PartialOrd, Copy, Clone)]
pub struct VPage(VPtr);

impl fmt::Debug for VPage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "VPage({:x?})", self.ptr().0)
    }
}

impl Add<usize> for VPage {
    type Output = Self;

    fn add(self, count: usize) -> VPage {
        VPage(self.ptr() + (count << abi::PAGE_SHIFT))
    }
}

impl Sub<usize> for VPage {
    type Output = Self;

    fn sub(self, count: usize) -> VPage {
        VPage(self.ptr() - (count << abi::PAGE_SHIFT))
    }
}

impl VPage {
    pub fn null() -> VPage {
        VPage(VPtr::null())
    }

    pub fn max() -> VPage {
        VPage::round_down(VPtr(usize::MAX))
    }

    pub fn task_max() -> VPage {
        VPage::round_down(VPtr(abi::TASK_SIZE))
    }

    pub fn task_unmapped_base() -> VPage {
        VPage::round_down(VPtr(VPage::task_max().ptr().0 / 3))
    }

    pub fn task_dyn_base() -> VPage {
        VPage::round_down(VPtr(VPage::task_unmapped_base().ptr().0 * 2))
    }

    pub fn offset(ptr: VPtr) -> usize {
        page_offset(ptr.0)
    }

    pub fn randomize(&self) -> VPage {
        const MASK: usize = ((1 << abi::MMAP_RND_BITS) - 1) & !(abi::PAGE_SIZE - 1);
        VPage(self.ptr() + (nolibc::getrandom_usize() & MASK))
    }

    pub fn ptr(&self) -> VPtr {
        self.0
    }

    pub fn round_up(ptr: VPtr) -> VPage {
        if 0 == VPage::offset(ptr) {
            VPage(ptr)
        } else {
            VPage::round_down(ptr) + 1
        }
    }

    pub fn round_down(ptr: VPtr) -> VPage {
        VPage(VPtr(ptr.0 & !(abi::PAGE_SIZE - 1)))
    }

    pub fn round_out(range: &Range<VPtr>) -> Range<VPage> {
        VPage::round_down(range.start)..VPage::round_up(range.end)
    }

    pub fn parse(ptr: VPtr) -> Result<VPage, ()> {
        if 0 == VPage::offset(ptr) {
            Ok(VPage(ptr))
        } else {
            Err(())
        }
    }

    pub fn parse_range(range: &Range<VPtr>) -> Result<Range<VPage>, ()> {
        Ok(VPage::parse(range.start)?..VPage::parse(range.end)?)
    }
}
