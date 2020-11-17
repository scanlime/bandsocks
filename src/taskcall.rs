use crate::{
    filesystem::vfs::{Filesystem, VFile},
    process::Process,
    sand::protocol::{Errno, SysFd, VString},
};
use std::path::Path;

fn user_string(process: &mut Process, s: VString) -> Result<String, Errno> {
    process.read_string(s).map_err(|_| Errno(-libc::EFAULT))
}

pub async fn chdir(
    process: &mut Process,
    _filesystem: &Filesystem,
    path: VString,
) -> Result<(), Errno> {
    let path = user_string(process, path)?;
    log::info!("chdir({:?})", path);
    Ok(())
}

pub async fn file_access(
    process: &mut Process,
    _filesystem: &Filesystem,
    dir: Option<SysFd>,
    path: VString,
    mode: i32,
) -> Result<(), Errno> {
    let path = user_string(process, path)?;
    log::info!("access({:?}, {:?}, {:?})", dir, path, mode);
    Err(Errno(-libc::ENOENT))
}

pub async fn file_open(
    process: &mut Process,
    filesystem: &Filesystem,
    dir: Option<SysFd>,
    path: VString,
    flags: i32,
    mode: i32,
) -> Result<VFile, Errno> {
    let path_str = user_string(process, path)?;
    let path = Path::new(&path_str);
    log::info!("open({:?}, {:?}, {:?}, {:?})", dir, path, flags, mode,);
    match filesystem.open(&path) {
        Err(e) => Err(Errno(-e.to_errno())),
        Ok(vfile) => {
            // to do: permissions
            Ok(vfile)
        }
    }
}
