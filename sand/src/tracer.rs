// This code may not be used for any purpose. Be gay, do crime.

use crate::{modes, exec_self};

pub fn run() {

    exec_self(modes::STAGE_2_LOADER);
    

    /*
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
*/
}
