use crate::protocol::{InitArgsHeader, SysFd};
use core::mem::size_of_val;
use sc::syscall;

fn read_header(fd: SysFd) -> InitArgsHeader {
    let mut header: InitArgsHeader = Default::default();
    let header_ptr = &mut header as *mut InitArgsHeader as *mut usize as usize;
    let header_len = size_of_val(&header);
    let result = unsafe { syscall!(READ, fd.0, header_ptr, header_len) as isize };
    assert_eq!(result, header_len as isize);
    header
}

pub fn with_args_from_fd(fd: SysFd) -> ! {
    let header = read_header(fd);

    println!("in the loader, {:?}", header);

    unsafe { syscall!(EXECVE, 0, 0, 0) };
    unreachable!();
}
