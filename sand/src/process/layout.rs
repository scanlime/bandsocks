use crate::{
    abi, nolibc,
    process::remote::Trampoline,
    protocol::{Errno, VPtr},
};
use sc::nr;

#[derive(Debug, Clone)]
pub struct MemLayout {
    pub start_code: VPtr,
    pub end_code: VPtr,
    pub start_data: VPtr,
    pub end_data: VPtr,
    pub start_stack: VPtr,
    pub start_brk: VPtr,
    pub brk: VPtr,
    pub arg_start: VPtr,
    pub arg_end: VPtr,
    pub env_start: VPtr,
    pub env_end: VPtr,
    pub auxv_ptr: VPtr,
    pub auxv_len: usize,
    pub exe_file: VPtr,
}

impl MemLayout {
    pub fn new(start_stack: VPtr) -> MemLayout {
        let null = VPtr(0);
        let ptr_max = VPtr(!0);
        MemLayout {
            start_code: ptr_max,
            end_code: null,
            start_data: ptr_max,
            end_data: null,
            start_stack,
            start_brk: null,
            brk: null,
            arg_start: null,
            arg_end: null,
            env_start: null,
            env_end: null,
            auxv_ptr: null,
            auxv_len: 0,
            exe_file: null,
        }
    }

    pub fn include_code(&mut self, addr: VPtr, length: usize) {
        self.start_code = self.start_code.min(addr);
        self.end_code = self.end_code.max(addr.add(length));
    }

    pub fn include_data(&mut self, addr: VPtr, length: usize) {
        self.start_data = self.start_data.min(addr);
        self.end_data = self.end_data.max(addr.add(length));
    }

    pub fn include_memory(&mut self, addr: VPtr, length: usize) {
        let end = addr.add(length);
        self.brk = self.brk.max(end);
        self.start_brk = self.start_brk.max(end);
    }

    pub fn randomize_brk(&mut self) {
        let random_offset = nolibc::getrandom_usize() & abi::BRK_RND_MASK;
        let brk = VPtr(abi::page_round_up(
            self.start_brk.0 + (random_offset << abi::PAGE_SHIFT),
        ));
        self.brk = brk;
        self.start_brk = brk;
    }

    pub async fn install(&self, trampoline: &mut Trampoline<'_, '_, '_>) -> Result<(), Errno> {
        for args in &[
            [
                abi::PR_SET_MM,
                abi::PR_SET_MM_START_CODE,
                self.start_code.0 as isize,
                0,
            ],
            [
                abi::PR_SET_MM,
                abi::PR_SET_MM_END_CODE,
                self.end_code.0 as isize,
                0,
            ],
            [
                abi::PR_SET_MM,
                abi::PR_SET_MM_START_DATA,
                self.start_data.0 as isize,
                0,
            ],
            [
                abi::PR_SET_MM,
                abi::PR_SET_MM_END_DATA,
                self.end_data.0 as isize,
                0,
            ],
            [
                abi::PR_SET_MM,
                abi::PR_SET_MM_START_STACK,
                self.start_stack.0 as isize,
                0,
            ],
            [
                abi::PR_SET_MM,
                abi::PR_SET_MM_START_BRK,
                self.start_brk.0 as isize,
                0,
            ],
            [
                abi::PR_SET_MM,
                abi::PR_SET_MM_BRK,
                self.brk.0 as isize,
                0isize,
            ],
            [
                abi::PR_SET_MM,
                abi::PR_SET_MM_ARG_START,
                self.arg_start.0 as isize,
                0,
            ],
            [
                abi::PR_SET_MM,
                abi::PR_SET_MM_ARG_END,
                self.arg_end.0 as isize,
                0,
            ],
            [
                abi::PR_SET_MM,
                abi::PR_SET_MM_ENV_START,
                self.env_start.0 as isize,
                0,
            ],
            [
                abi::PR_SET_MM,
                abi::PR_SET_MM_ENV_END,
                self.env_end.0 as isize,
                0,
            ],
            [
                abi::PR_SET_MM,
                abi::PR_SET_MM_EXE_FILE,
                self.exe_file.0 as isize,
                0,
            ],
            [
                abi::PR_SET_MM,
                abi::PR_SET_MM_AUXV,
                self.auxv_ptr.0 as isize,
                self.auxv_len as isize,
            ],
        ] {
            let result = trampoline.syscall(nr::PRCTL, args).await;
            println!("prctl arg {:x?} {}", args, result);
            if result != 0 {
                return Err(Errno(result as i32));
            }
        }
        Ok(())
    }
}
