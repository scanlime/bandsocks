use crate::{nolibc::File, protocol::InitArgsHeader};
use alloc::vec::Vec;
use sc::syscall;

fn read_header(file: &File) -> InitArgsHeader {
    let mut header: InitArgsHeader = Default::default();
    file.read_exact(header.as_bytes_mut()).unwrap();
    header
}

pub fn with_args_file(file: &File) -> ! {
    let header = read_header(file);

    let mut bytes = Vec::<u8>::new();
    bytes.resize(
        header.dir_len + header.filename_len + header.argv_len + header.envp_len,
        0,
    );

    file.read_exact(&mut bytes).unwrap();
    file.close().unwrap();

    let (dir, bytes) = bytes.split_at(header.dir_len);
    let (filename, bytes) = bytes.split_at(header.filename_len);
    let (argv, bytes) = bytes.split_at(header.argv_len);
    let (envp, bytes) = bytes.split_at(header.envp_len);
    assert_eq!(bytes.len(), 0);

    let mut pointers = Vec::<usize>::new();
    pointers.resize(header.arg_count + 1 + header.env_count + 1, 0);

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
}

fn cstr_vec_pointers(cstr_vec: &[u8], count: usize, pointers: &mut [usize]) {
    let mut offset = 0;
    for pointer in pointers[..count].iter_mut() {
        *pointer = cstr_vec[offset..].as_ptr() as usize;
        while cstr_vec[offset] != 0 {
            offset += 1;
        }
        offset += 1;
    }
    assert_eq!([0], cstr_vec[offset..]);
    assert_eq!(pointers.len(), count + 1);
    pointers[count] = 0;
}
