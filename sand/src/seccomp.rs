// This code may not be used for any purpose. Be gay, do crime.

use crate::abi;
use sc::syscall;

pub fn activate() {
    let filter = [ 0 as u64 ];
    let result = unsafe {
        syscall!(PRCTL, abi::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
        syscall!(PRCTL, abi::PR_SET_SECCOMP, abi::SECCOMP_MODE_FILTER, filter.as_ptr() as usize, 0, 0) as isize
    };
    if result != 0 {
        panic!("seccomp setup error ({})", result);
    }
}
