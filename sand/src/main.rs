// This code may not be used for any purpose. Be gay, do crime.

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

use pentacle::ensure_sealed;
use pete::{Command, Ptracer, Restart};
use syscallz::{Context, Action, Syscall};

fn main() {
    ensure_sealed().unwrap();
    println!("sand {:?}", std::env::args());

    if let Some(argv0) = std::env::args().next() {
        if argv0 == "sand" {
            do_seccomp();
            do_ptrace();
        } else {
            do_shell();
        }
    }
}

fn do_shell() {
    std::process::Command::new("/bin/sh").status().unwrap();
}

fn do_ptrace() {
    let argv = vec![ b"/proc/self/exe".to_vec() ];
    let cmd = Command::new(argv).unwrap();
    let mut ptracer = Ptracer::new();

    let tracee = ptracer.spawn(cmd).unwrap();
    ptracer.restart(tracee, Restart::Continue).unwrap();

    while let Some(tracee) = ptracer.wait().unwrap() {
        let regs = tracee.registers().unwrap();
        let pc = regs.rip as u64;

        println!("{:>16x}: {:?}", pc, tracee.stop);

        ptracer.restart(tracee, Restart::Continue).unwrap();
    }
}

fn do_seccomp() {
    let mut ctx = Context::init_with_action(Action::Allow).unwrap();
    ctx.set_action_for_syscall(Action::Trace(1234), Syscall::uname);

    println!("pre-seccomp");
    ctx.load().unwrap();
    println!("post-seccomp");
}
