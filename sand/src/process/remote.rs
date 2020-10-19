use crate::{
    abi,
    abi::SyscallInfo,
    process::{task::StoppedTask, Event},
    ptrace,
};

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
    SyscallInfo::ret_from_regs(regs)
}
