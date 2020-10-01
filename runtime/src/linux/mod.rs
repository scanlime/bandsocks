// This code may not be used for any purpose. Be gay, do crime.

use pentacle::SealedCommand;
use pete::{Command, Ptracer, Restart};
use std::io::Cursor;
use std::os::unix::process::CommandExt;

pub fn do_the_thing() {

    const ELF: &'static [u8] = include_bytes!(concat!(
        env!("OUT_DIR"), "/sand-target/release/bandsocks-sand"));

    let mut elf_reader = Cursor::new(ELF);
    let mut cmd = SealedCommand::new(&mut elf_reader).unwrap();
    cmd.arg0("sand");
    println!("{:?}", cmd.status());

    /*    
    let argv = vec![ b"/dev/null".to_vec() ];
    let cmd = Command::new(argv).unwrap();
    let mut ptracer = Ptracer::new();

    let tracee = ptracer.spawn(cmd).unwrap();
    ptracer.restart(tracee, Restart::Continue).unwrap();

    while let Some(tracee) = ptracer.wait().unwrap() {
        let regs = tracee.registers().unwrap();
        let pc = regs.rip as u64;

        println!("{:>16x}: {:?}", pc, tracee.stop);

        ptracer.restart(tracee, Restart::Continue).unwrap();
    */
}

