// This code may not be used for any purpose. Be gay, do crime.

use core::mem;
use core::ptr::null;
use core::default::Default;
use sc::syscall;
use crate::abi;

pub struct Tracer {
}

impl Tracer {
    pub fn new() -> Self {
        Tracer {
        }
    }

    pub fn spawn(&self, cmd: &[u8], argv: &[*const u8], envp: &[*const u8]) {
        unsafe {
            match syscall!(FORK) as isize {
                err if err < 0 => panic!("fork error"),
                pid if pid == 0 => {
                    syscall!(EXECVE, cmd.as_ptr(), argv.as_ptr(), envp.as_ptr());
                    panic!("exec failed");
                }
                pid => pid,
            }
        };
    }

    pub fn handle_events(&mut self) {
        let mut info: abi::SigInfo = Default::default();
        let info_ptr = &mut info as *mut abi::SigInfo as usize;
        assert_eq!(mem::size_of_val(&info), abi::SI_MAX_SIZE);

        loop {
            let which = abi::P_ALL;
            let pid = -1 as isize as usize;
            let options = abi::WEXITED | abi::WSTOPPED | abi::WCONTINUED;
            let rusage = null::<usize>() as usize;
            let result = unsafe { syscall!(WAITID, which, pid, info_ptr, options, rusage) as isize };
            match result {
                0 => self.handle_event(&info),
                abi::ECHILD => {
                    // No more child processes
                    break;
                },
                other => {
                    panic!("waitid err ({})", other);
                }
            }
        }
    }

    fn handle_event(&mut self, info: &abi::SigInfo) {
        println!("tracer woke up. {:?}", info);
    }
}
