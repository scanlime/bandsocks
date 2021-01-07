use crate::{
    filesystem::vfs::Filesystem,
    process::Process,
    sand::protocol::{Errno, FileStat, FollowLinks, VFile, VString},
};
use std::{ffi::CString, path::Path};

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
) -> Result<CString, Errno> {
    Ok(CString::new("working dir goes here").unwrap())
}

pub async fn readlink(
    process: &mut Process,
    filesystem: &Filesystem,
    path: &VString,
) -> Result<CString, Errno> {
    let path_str = process.mem.read_user_string(path)?;
    let path = Path::new(&path_str);
    let dir = &process.status.current_dir;
    let vfile = filesystem.lookup(dir, &path, &FollowLinks::NoFollow)?;
    let cstr = filesystem.readlink(&vfile)?;
    log::debug!("readlink({:?}) -> {:?}", path, cstr);
    Ok(cstr.to_owned())
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
    let vfile = filesystem.lookup(&dir, &path, &FollowLinks::Follow)?;
    log::debug!("file_open{:?} -> {:?}", (dir, path, flags, mode), vfile);
    Ok(vfile)
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
        Some(path) => filesystem.lookup(file, path, follow_links)?,
    };
    let stat = filesystem.stat(&file)?.to_owned();
    log::debug!(
        "file_stat{:?} -> {:?}",
        (path, follow_links),
        (&file, &stat)
    );
    Ok((file, stat))
}
