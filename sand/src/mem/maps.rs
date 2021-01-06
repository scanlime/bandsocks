use crate::{
    abi,
    mem::page::{page_offset, VPage},
    protocol::VPtr,
};
use core::{fmt, ops::Range};

/// A range of virtual address space that corresponds to file data
#[derive(PartialEq, Eq, Clone)]
pub struct MappedRange {
    pub mem: Range<VPtr>,
    pub file_start: usize,
}

impl fmt::Debug for MappedRange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "MappedRange({:x?}..{:x?} @{:x?})",
            self.mem.start.0, self.mem.end.0, self.file_start
        )
    }
}

/// A MappedRange that must be page-aligned
#[derive(PartialEq, Eq, Clone)]
pub struct MappedPages {
    mem: Range<VPage>,
    file_start: usize,
}

impl fmt::Debug for MappedPages {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let range = self.mapped_range();
        write!(
            f,
            "MappedPages({:x?}..{:x?} @{:x?})",
            range.mem.start.0, range.mem.end.0, range.file_start
        )
    }
}

impl MappedPages {
    pub fn mapped_range(&self) -> MappedRange {
        MappedRange {
            mem: self.mem_range(),
            file_start: self.file_start,
        }
    }

    pub fn mem_pages(&self) -> Range<VPage> {
        self.mem.clone()
    }

    pub fn mem_range(&self) -> Range<VPtr> {
        self.mem.start.ptr()..self.mem.end.ptr()
    }

    pub fn file_start(&self) -> usize {
        self.file_start
    }

    pub fn round_out(range: &MappedRange) -> MappedPages {
        let mem = VPage::round_out(&range.mem);
        let start_offset = range.mem.start.0 - mem.start.ptr().0;
        let file_start = range.file_start - start_offset;
        MappedPages { mem, file_start }
    }

    pub fn anonymous(mem: Range<VPage>) -> MappedPages {
        MappedPages { mem, file_start: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.mem.is_empty()
    }

    pub fn is_overlap(&self, other: &Self) -> bool {
        if self.is_empty() || other.is_empty() {
            false
        } else {
            let min_end = self.mem_pages().end.min(other.mem_pages().end);
            let max_start = self.mem_pages().start.max(other.mem_pages().start);
            max_start < min_end
        }
    }

    pub fn parse(range: &MappedRange) -> Result<MappedPages, ()> {
        let file_start = range.file_start;
        if 0 == page_offset(file_start) {
            let mem = VPage::parse_range(&range.mem)?;
            Ok(MappedPages { mem, file_start })
        } else {
            Err(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub mapped_range: MappedRange,
    pub mem_size: usize,
    pub protect: MemProtect,
}

impl Segment {
    pub fn mem_range(&self) -> Range<VPtr> {
        self.mapped_range.mem.start..(self.mapped_range.mem.end + self.mem_size)
    }

    pub fn mem_pages(&self) -> Range<VPage> {
        VPage::round_out(&self.mem_range())
    }

    pub fn mapped_pages(&self) -> MappedPages {
        MappedPages::round_out(&self.mapped_range)
    }

    pub fn set_range_start(mut self, addr: VPtr) -> Self {
        let mapped_len = self.mapped_range.mem.end.0 - self.mapped_range.mem.start.0;
        self.mapped_range.mem.start = addr;
        self.mapped_range.mem.end = addr + mapped_len;
        self
    }

    pub fn set_page_start(self, addr: VPage) -> Self {
        let offset = VPage::offset(self.mapped_range.mem.start);
        self.set_range_start(addr.ptr() + offset)
    }

    pub fn offset(self, offset: usize) -> Self {
        let start = self.mapped_range.mem.start + offset;
        self.set_range_start(start)
    }
}

#[derive(Eq, PartialEq, Clone)]
pub struct MemProtect {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl core::fmt::Debug for MemProtect {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{}{}{}",
            if self.read { "r" } else { "-" },
            if self.write { "w" } else { "-" },
            if self.execute { "x" } else { "-" },
        )
    }
}

impl MemProtect {
    pub fn prot_flags(&self) -> isize {
        (if self.read { abi::PROT_READ } else { 0 })
            | (if self.write { abi::PROT_WRITE } else { 0 })
            | (if self.execute { abi::PROT_EXEC } else { 0 })
    }
}

#[derive(Eq, PartialEq, Clone)]
pub struct MemFlags {
    pub protect: MemProtect,
    pub mayshare: bool,
}

impl fmt::Debug for MemFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{:?}{}",
            self.protect,
            if self.mayshare { "s" } else { "p" },
        )
    }
}

impl MemFlags {
    pub fn ro() -> MemFlags {
        MemFlags {
            protect: MemProtect {
                read: true,
                write: false,
                execute: false,
            },
            mayshare: false,
        }
    }

    pub fn rw() -> MemFlags {
        MemFlags {
            protect: MemProtect {
                read: true,
                write: true,
                execute: false,
            },
            mayshare: false,
        }
    }

    pub fn exec() -> MemFlags {
        MemFlags {
            protect: MemProtect {
                read: true,
                write: false,
                execute: true,
            },
            mayshare: false,
        }
    }

    pub fn map_flags(&self) -> isize {
        if self.mayshare {
            abi::MAP_SHARED
        } else {
            abi::MAP_PRIVATE
        }
    }
}
