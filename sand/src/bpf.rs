// This code may not be used for any purpose. Be gay, do crime.

use core::convert::TryInto;
use core::marker::PhantomData;
use crate::abi::*;

const SIZE_LIMIT: usize = 4096;

pub struct ProgramBuffer {
    len: u16,
    array: [SockFilter; SIZE_LIMIT],
}

impl ProgramBuffer {
    pub fn new() -> Self {
        const EMPTY: SockFilter = SockFilter {
            code: 0, k: 0, jt: 0, jf: 0
        };
        ProgramBuffer {
            len: 0,
            array: [ EMPTY; SIZE_LIMIT ]
        }
    }

    pub fn to_filter_prog<'a>(&'a self) -> SockFilterProg<'a> {
        SockFilterProg {
            len: self.len,
            filter: self.array.as_ptr(),
            phantom: PhantomData
        }
    }

    pub fn block(&mut self, block: &[SockFilter]) {
        for instruction in block {
            self.inst(*instruction);
        }
    }

    pub fn inst(&mut self, instruction: SockFilter) {
        if self.len as usize == SIZE_LIMIT {
            panic!("filter program exceeding size limit");
        }
        self.array[self.len as usize] = instruction;
        self.len += 1;
    }
    
    pub fn if_eq(&mut self, k: usize, block: &[SockFilter]) {
        let offset: u8 = block.len().try_into().unwrap();
        self.inst(jump( BPF_JMP+BPF_JEQ+BPF_K, k as u32, 0, offset ));
        self.block(block);
    }    
}

pub const fn stmt(code: u16, k: u32) -> SockFilter {
    SockFilter { code, k, jt: 0, jf: 0 }
}

pub const fn jump(code: u16, k: u32, jt: u8, jf: u8) -> SockFilter {
    SockFilter { code, k, jt, jf }
}

pub const fn imm(k: u32) -> SockFilter {
    stmt( BPF_LD+BPF_W+BPF_IMM, k )
}

pub const fn ret(k: u32) -> SockFilter {
    stmt( BPF_RET+BPF_K, k )
}

pub const fn load(k: usize) -> SockFilter {
    stmt( BPF_LD+BPF_W+BPF_ABS, k as u32 )
}

pub const fn store(k: usize) -> SockFilter {
    stmt( BPF_ST, k as u32 )
}
