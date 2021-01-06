use crate::{abi, protocol::VPtr};
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

    pub fn offset(ptr: VPtr) -> usize {
        page_offset(ptr.0)
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
