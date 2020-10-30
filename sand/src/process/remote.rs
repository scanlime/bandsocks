use crate::{
    abi,
    abi::SyscallInfo,
    process::{task::StoppedTask, Event},
    protocol::VPtr,
    ptrace,
};
use heapless::Vec;
use sc::{syscall};
use typenum::*;

pub async fn syscall<'q, 's>(
    stopped_task: &mut StoppedTask<'q, 's>,
    nr: usize,
    args: &[isize],
) -> isize {
    let task = &mut stopped_task.task;
    let regs = &mut stopped_task.regs;
    let pid = task.task_data.sys_pid;

    SyscallInfo::orig_nr_to_regs(nr as isize, regs);
    SyscallInfo::args_to_regs(args, regs);
    println!(">>> pre-rsyscall {:x?}", regs);
    ptrace::set_regs(pid, regs);

    ptrace::trace_syscall(pid);
    assert_eq!(
        task.events.next().await,
        Event::Signal {
            sig: abi::SIGCHLD,
            code: abi::CLD_TRAPPED,
            status: abi::PTRACE_SIG_TRACESYSGOOD
        }
    );

    ptrace::get_regs(pid, regs);
    println!("<<< post-rsyscall {:x?}", regs);
    SyscallInfo::ret_from_regs(regs)
}

pub fn mem_write<'q, 's>(
    stopped_task: &mut StoppedTask<'q, 's>,
    ptr: VPtr,
    bytes: &[u8],
) -> Result<(), ()> {
    let mem_fd = stopped_task.task.process_handle.mem.0;
    match unsafe {
        ptr.0 == syscall!(LSEEK, mem_fd, ptr.0, abi::SEEK_SET)
            && bytes.len() == syscall!(WRITE, mem_fd, bytes.as_ptr() as usize, bytes.len())
    } {
        false => Err(()),
        true => Ok(())
    }
}

pub fn print_maps(stopped_task: &mut StoppedTask) {
    let maps_fd = stopped_task.task.process_handle.maps.0;
    let mut buf: Vec<u8, U8192> = Vec::new();
    unsafe {
        assert_eq!(0, syscall!(LSEEK, maps_fd, 0, abi::SEEK_SET));
        let len = syscall!(READ, maps_fd, buf.as_mut_ptr(), buf.capacity()) as isize;
        assert!(len >= 0 && len as usize <= buf.capacity());
        buf.set_len(len as usize);
    };
    let maps_str = core::str::from_utf8(&buf);
    println!("maps: {:?}", maps_str);
}
