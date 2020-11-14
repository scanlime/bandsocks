use crate::protocol::{InitArgsHeader, SysFd};
use sc::syscall;
use smalloca::smalloca_default;

fn read_header(fd: &SysFd) -> InitArgsHeader {
    let mut header: InitArgsHeader = Default::default();
    fd.read_exact(header.as_bytes_mut()).unwrap();
    header
}

pub fn with_args_from_fd(fd: &SysFd) -> ! {
    let header = read_header(fd);
    // alloca() one buffer to hold the text content we are reading
    smalloca_default(
        header.dir_len + header.filename_len + header.argv_len + header.envp_len,
        |bytes: &mut [u8]| {
            fd.read_exact(bytes).unwrap();
            let (dir, bytes) = bytes.split_at(header.dir_len);
            let (filename, bytes) = bytes.split_at(header.filename_len);
            let (argv, bytes) = bytes.split_at(header.argv_len);
            let (envp, bytes) = bytes.split_at(header.envp_len);
            assert_eq!(bytes.len(), 0);
            // alloca() a second buffer for the pointers we pass to exec
            smalloca_default(
                header.arg_count + 1 + header.env_count + 1,
                |pointers: &mut [usize]| {
                    let (argv_ptrs, pointers) = pointers.split_at_mut(header.arg_count + 1);
                    let (envp_ptrs, pointers) = pointers.split_at_mut(header.env_count + 1);
                    assert_eq!(pointers.len(), 0);

                    cstr_vec_pointers(argv, header.arg_count, argv_ptrs);
                    cstr_vec_pointers(envp, header.env_count, envp_ptrs);

                    // change directories
                    if 0 != unsafe { syscall!(CHDIR, dir.as_ptr()) } {
                        panic!("failed to change to startup directory");
                    }

                    // now let the emulated kernel take over
                    let error = unsafe {
                        syscall!(
                            EXECVE,
                            filename.as_ptr(),
                            argv_ptrs.as_ptr(),
                            envp_ptrs.as_ptr()
                        ) as isize
                    };
                    panic!("initial exec failed ({})", error);
                },
            );
        },
    );
    unreachable!();
}

fn cstr_vec_pointers(cstr_vec: &[u8], count: usize, pointers: &mut [usize]) {
    let mut offset = 0;
    for index in 0..count {
        pointers[index] = cstr_vec[offset..].as_ptr() as usize;
        while cstr_vec[offset] != 0 {
            offset += 1;
        }
        offset += 1;
    }
    assert_eq!([0], cstr_vec[offset..]);
    assert_eq!(pointers.len(), count + 1);
    pointers[count] = 0;
}
