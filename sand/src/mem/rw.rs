use crate::{
    abi,
    nolibc::File,
    process::task::StoppedTask,
    protocol::{Errno, VPtr},
    ptrace,
};
use core::{
    mem::{size_of, MaybeUninit},
    ops::Range,
    slice,
};
use typenum::*;

pub fn read_bytes(
    stopped_task: &mut StoppedTask,
    ptr: VPtr,
    bytes: &mut [u8],
) -> Result<(), Errno> {
    let mem_file = File::new(stopped_task.task.process_handle.mem);
    mem_file
        .pread_exact(bytes, ptr.0)
        .map_err(|_| Errno(-abi::EFAULT))
}

/// safety: type must be repr(C) and have no invalid bit patterns
pub unsafe fn read_value<T: Clone>(
    stopped_task: &mut StoppedTask,
    remote: VPtr,
) -> Result<T, Errno> {
    let len = size_of::<T>();
    let mut storage = MaybeUninit::<T>::uninit();
    let byte_ref = slice::from_raw_parts_mut(&mut storage as *mut MaybeUninit<T> as *mut u8, len);
    read_bytes(stopped_task, remote, byte_ref)?;
    let value_ref: &mut T = &mut *(byte_ref.as_mut_ptr() as *mut T);
    Ok(value_ref.clone())
}

pub fn read_word(stopped_task: &mut StoppedTask, remote: VPtr) -> Result<usize, Errno> {
    unsafe { read_value(stopped_task, remote) }
}

pub fn read_pointer(stopped_task: &mut StoppedTask, remote: VPtr) -> Result<VPtr, Errno> {
    unsafe { read_value(stopped_task, remote) }
}

pub fn write_word(stopped_task: &mut StoppedTask, ptr: VPtr, word: usize) -> Result<(), Errno> {
    assert!(0 == (ptr.0 % size_of::<usize>()));
    let result = ptrace::poke(stopped_task.task.task_data.sys_pid, ptr.0, word)
        .map_err(|()| Errno(-abi::EFAULT));
    result
}

pub fn write_padded_bytes(
    stopped_task: &mut StoppedTask,
    mut ptr: VPtr,
    bytes: &[u8],
) -> Result<(), Errno> {
    assert!(0 == (ptr.0 % size_of::<usize>()));
    for chunk in bytes.chunks(size_of::<usize>()) {
        let mut padded_chunk = 0usize.to_ne_bytes();
        padded_chunk[0..chunk.len()].copy_from_slice(chunk);
        write_word(stopped_task, ptr, usize::from_ne_bytes(padded_chunk))?;
        ptr = ptr + size_of::<usize>();
    }
    write_word(stopped_task, ptr, 0)?;
    Ok(())
}

/// safety: type must be repr(C)
pub unsafe fn write_padded_value<T: Clone>(
    stopped_task: &mut StoppedTask,
    remote: VPtr,
    local: &T,
) -> Result<(), Errno> {
    // allocate aligned for T, explicitly zero all bytes, clone the value in, then
    // use as bytes again
    let len = size_of::<T>();
    let mut storage = MaybeUninit::<T>::uninit();
    let byte_ref = slice::from_raw_parts_mut(&mut storage as *mut MaybeUninit<T> as *mut u8, len);
    for byte in byte_ref.iter_mut() {
        *byte = 0;
    }
    let value_ref: &mut T = &mut *(byte_ref.as_mut_ptr() as *mut T);
    value_ref.clone_from(local);
    let byte_ref = slice::from_raw_parts(value_ref as *mut T as *mut u8, len);
    write_padded_bytes(stopped_task, remote, byte_ref)
}

pub fn find_bytes(
    stopped_task: &mut StoppedTask,
    area: Range<VPtr>,
    pattern: &[u8],
) -> Result<Option<VPtr>, Errno> {
    type BufSize = U256;
    let mut buffer = [0u8; BufSize::USIZE];
    assert!(pattern.len() <= buffer.len());
    if area.end > area.start {
        let mut ptr = area.start;
        loop {
            let chunk_size = buffer.len().min(area.end.0 - ptr.0);
            if chunk_size < pattern.len() {
                break;
            }
            read_bytes(stopped_task, ptr, &mut buffer)?;
            if let Some(match_offset) = twoway::find_bytes(&buffer, pattern) {
                return Ok(Some(ptr + match_offset));
            }
            // overlap just enough to detect matches across chunk boundaries
            ptr = ptr + (chunk_size - pattern.len() + 1);
        }
    }
    Ok(None)
}

pub fn find_syscall(stopped_task: &mut StoppedTask, area: Range<VPtr>) -> Result<VPtr, Errno> {
    match find_bytes(stopped_task, area, &abi::SYSCALL_INSTRUCTION)? {
        Some(ptr) => Ok(ptr),
        None => Err(Errno(-abi::ENOSYS)),
    }
}

pub fn print_stack_dump(stopped_task: &mut StoppedTask) {
    println!("stack dump:");
    let mut sp = VPtr(stopped_task.regs.sp);
    let mut previous_word = None;
    let mut skipping = false;
    while let Ok(word) = read_word(stopped_task, sp) {
        if Some(word) == previous_word {
            if !skipping {
                println!("...");
                skipping = true;
            }
        } else {
            skipping = false;
            previous_word = Some(word);
            print!("{:x?} = {:16x}  ", sp, word,);
            for byte in &word.to_ne_bytes() {
                if byte.is_ascii_graphic() || *byte == b' ' {
                    print!("{}", *byte as char);
                } else {
                    print!(".");
                }
            }
            println!();
        }
        sp = sp + size_of::<usize>();
    }
}
