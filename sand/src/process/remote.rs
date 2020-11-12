use crate::{
    abi,
    abi::SyscallInfo,
    process::{maps::MapsIterator, task::StoppedTask, Event},
    protocol::VPtr,
    ptrace,
};
use sc::syscall;

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

pub fn mem_read<'q, 's>(
    stopped_task: &mut StoppedTask<'q, 's>,
    ptr: VPtr,
    bytes: &mut [u8],
) -> Result<(), ()> {
    let mem_fd = stopped_task.task.process_handle.mem.0;
    match unsafe {
        bytes.len() == syscall!(PREAD64, mem_fd, bytes.as_mut_ptr() as usize, bytes.len(), ptr.0)
    } {
        false => Err(()),
        true => Ok(()),
    }
}

pub fn mem_write<'q, 's>(
    stopped_task: &mut StoppedTask<'q, 's>,
    ptr: VPtr,
    bytes: &[u8],
) -> Result<(), ()> {
    let mem_fd = stopped_task.task.process_handle.mem.0;
    match unsafe {
        bytes.len() == syscall!(PWRITE64, mem_fd, bytes.as_ptr() as usize, bytes.len(), ptr.0)
    } {
        false => Err(()),
        true => Ok(()),
    }
}

pub fn print_maps(stopped_task: &mut StoppedTask) {
    for map in &mut MapsIterator::new(stopped_task) {
        println!("{:x?}", map);
    }
}
