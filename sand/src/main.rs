// This code may not be used for any purpose. Be gay, do crime.

use pentacle::ensure_sealed;
use pete::{Command, Ptracer, Restart};

fn main() {
    ensure_sealed().unwrap();

    println!("sand {:?}", std::env::args());
    std::thread::sleep(std::time::Duration::from_secs(1));

    if let Some(argv0) = std::env::args().next() {

        if argv0 == "sand" {
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
