use crate::{
    filesystem::vfs::Filesystem,
    process::Process,
    sand::protocol::{Errno, FileStat, FollowLinks, VFile, VString, VStringBuffer},
};
use std::path::Path;

pub async fn change_working_dir(
    process: &mut Process,
    _filesystem: &Filesystem,
    path: &VString,
) -> Result<(), Errno> {
    let path = process.mem.read_user_string(path)?;
    log::warn!("change_working_dir({:?})", path);
    Ok(())
}

pub async fn get_working_dir(
    _process: &mut Process,
    _filesystem: &Filesystem,
    buffer: &VStringBuffer,
) -> Result<usize, Errno> {
    log::warn!("get_working_dir({:x?})", buffer);
    Ok(0)
}

pub async fn readlink(
    process: &mut Process,
    _filesystem: &Filesystem,
    path: &VString,
    buffer: &VStringBuffer,
) -> Result<usize, Errno> {
    let path = process.mem.read_user_string(path)?;
    log::warn!("readlink({:x?}, {:x?})", path, buffer);
    Err(Errno(-libc::EINVAL))
}

pub async fn file_open(
    process: &mut Process,
    filesystem: &Filesystem,
    dir: &Option<VFile>,
    path: &VString,
    flags: i32,
    mode: i32,
) -> Result<VFile, Errno> {
    let path_str = process.mem.read_user_string(path)?;
    let path = Path::new(&path_str);
    let dir = match dir {
        Some(dir) => &dir,
        None => &process.status.current_dir,
    };
    let result = filesystem.lookup(&dir, &path, &FollowLinks::Follow);
    log::debug!(
        "file_open({:?}, {:?}, {:?}, {:?}) -> {:?}",
        dir,
        path,
        flags,
        mode,
        result
    );
    match result {
        Err(e) => Err(Errno(-e.to_errno())),
        Ok(vfile) => Ok(vfile),
    }
}

pub async fn file_stat(
    process: &mut Process,
    filesystem: &Filesystem,
    file: &Option<VFile>,
    path: &Option<VString>,
    follow_links: &FollowLinks,
) -> Result<(VFile, FileStat), Errno> {
    let path = match path {
        Some(path) => {
            let path_str = process.mem.read_user_string(path)?;
            let path = Path::new(&path_str);
            Some(path.to_owned())
        }
        None => None,
    };
    let file = match file {
        Some(file) => &file,
        None => &process.status.current_dir,
    };
    let file = match &path {
        None => file.to_owned(),
        Some(path) => match filesystem.lookup(file, path, follow_links) {
            Ok(file) => file,
            Err(e) => return Err(Errno(-e.to_errno())),
        },
    };
    let stat = match filesystem.stat(&file) {
        Ok(stat) => stat.to_owned(),
        Err(e) => return Err(Errno(-e.to_errno())),
    };
    log::debug!(
        "file_stat({:?}, {:?}, {:?}) -> {:?}, {:?}",
        file,
        path,
        follow_links,
        file,
        stat
    );
    Ok((file, stat))
}
