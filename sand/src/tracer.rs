// This code may not be used for any purpose. Be gay, do crime.

/*
struct Tracer {
    loader_path: &'static [u8],
    loader_argv: &'static [*const u8],
    

    
const SELF_EXE: &'static str = "/proc/self/exe";

fn tracer_main() {
}


fn exec_inner() {
    let exe = CString::new("/bin/sh").unwrap();

    let argv: Vec<*const c_char> = vec![ exe.as_ptr(), null() ];
    let envp: Vec<*const c_char> = vec![ null() ];
    
    let result = unsafe { libc::execve(exe.as_ptr(), argv.as_ptr(), envp.as_ptr()) };
    panic!("sand: exec_inner fault ({})", result);
}

fn interactive_startup() {
    // Started under unknown conditions... this shouldn't happen when we're in the
    // runtime, but this is where we end up when running the binary manually for testing.
    // Restart as the stage 1 tracer.
    
    println!("hi.");
    exec_self(modes::STAGE_1_TRACER);
}

/*

fn fork_and_exec_next_stage() 
    let child_pid = unsafe { match libc::fork() {
        0 => {
        },
        pid => pid
    };

                             exec_self(modes::STAGE_2_LOADER);

pub fn run() {
*/
    

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
*/
