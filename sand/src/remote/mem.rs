use crate::{
    abi, nolibc,
    process::task::StoppedTask,
    protocol::{Errno, VString, VPtr},
    ptrace,
};
use core::mem::{size_of, MaybeUninit};

pub fn fault_or<T>(result: Result<T, ()>) -> Result<T, Errno> {
    match result {
        Ok(t) => Ok(t),
        Err(()) => Err(Errno(-abi::EFAULT)),
    }
}

pub fn read_bytes(
    stopped_task: &mut StoppedTask,
    ptr: VPtr,
    bytes: &mut [u8],
) -> Result<(), ()> {
    nolibc::pread_exact(&stopped_task.task.process_handle.mem, bytes, ptr.0)
}

/// safety: type must be repr(C) and have no invalid bit patterns
pub unsafe fn read_value<T: Clone>(stopped_task: &mut StoppedTask, remote: VPtr) -> Result<T, ()> {
    let len = size_of::<T>();
    let mut storage = MaybeUninit::<T>::uninit();
    let byte_ref =
        core::slice::from_raw_parts_mut(&mut storage as *mut MaybeUninit<T> as *mut u8, len);
    read_bytes(stopped_task, remote, byte_ref)?;
    let value_ref: &mut T = &mut *(byte_ref.as_mut_ptr() as *mut T);
    Ok(value_ref.clone())
}

pub fn read_pointer(stopped_task: &mut StoppedTask, remote: VPtr) -> Result<VPtr, ()> {
    unsafe { read_value(stopped_task, remote) }
}

pub fn read_pointer_array(stopped_task: &mut StoppedTask, array: VPtr, idx: usize) -> Result<VPtr, ()> {
    read_pointer(stopped_task, array.add(idx * size_of::<VPtr>()))
}

pub fn read_string_array(stopped_task: &mut StoppedTask, array: VPtr, idx: usize) -> Result<Option<VString>, ()> {
    match read_pointer_array(stopped_task, array, idx) {
        Err(()) => Err(()),
        Ok(ptr) if ptr == VPtr::null() => Ok(None),
        Ok(ptr) => Ok(Some(VString(ptr)))
    }
}

pub fn write_word(
    stopped_task: &mut StoppedTask,
    ptr: VPtr,
    word: usize,
) -> Result<(), ()> {
    assert!(0 == (ptr.0 % size_of::<usize>()));
    ptrace::poke(stopped_task.task.task_data.sys_pid, ptr.0, word)
}

pub fn write_padded_bytes(
    stopped_task: &mut StoppedTask,
    mut ptr: VPtr,
    bytes: &[u8],
) -> Result<(), ()> {
    assert!(0 == (ptr.0 % size_of::<usize>()));
    for chunk in bytes.chunks(size_of::<usize>()) {
        let mut padded_chunk = 0usize.to_ne_bytes();
        padded_chunk[0..chunk.len()].copy_from_slice(chunk);
        write_word(stopped_task, ptr, usize::from_ne_bytes(padded_chunk))?;
        ptr = ptr.add(size_of::<usize>());
    }
    write_word(stopped_task, ptr, 0)?;
    Ok(())
}

/// safety: type must be repr(C)
pub unsafe fn write_padded_value<T: Clone>(
    stopped_task: &mut StoppedTask,
    remote: VPtr,
    local: &T,
) -> Result<(), ()> {
    // allocate aligned for T, explicitly zero all bytes, clone the value in, then
    // use as bytes again
    let len = size_of::<T>();
    let mut storage = MaybeUninit::<T>::uninit();
    let byte_ref =
        core::slice::from_raw_parts_mut(&mut storage as *mut MaybeUninit<T> as *mut u8, len);
    for byte in byte_ref.iter_mut() {
        *byte = 0;
    }
    let value_ref: &mut T = &mut *(byte_ref.as_mut_ptr() as *mut T);
    value_ref.clone_from(local);
    let byte_ref = core::slice::from_raw_parts(value_ref as *mut T as *mut u8, len);
    write_padded_bytes(stopped_task, remote, byte_ref)
}

pub fn find_bytes(
    stopped_task: &mut StoppedTask,
    ptr: VPtr,
    len: usize,
    pattern: &[u8],
) -> Result<VPtr, ()> {
    let mut buffer = [0u8; 4096];
    assert!(pattern.len() <= buffer.len());

    let mut chunk_offset = 0;
    loop {
        let chunk_size = buffer.len().min(len - chunk_offset);
        if chunk_size < pattern.len() {
            return Err(());
        }
        read_bytes(stopped_task, ptr.add(chunk_offset), &mut buffer)?;
        if let Some(match_offset) = twoway::find_bytes(&buffer, pattern) {
            return Ok(ptr.add(chunk_offset).add(match_offset));
        }
        // overlap just enough to detect matches across chunk boundaries
        chunk_offset += chunk_size - pattern.len() + 1;
    }
}
