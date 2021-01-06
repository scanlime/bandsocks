pub mod elf64;
pub mod script;

use crate::{
    abi,
    mem::string::VStringArray,
    nolibc::{File, TempFile},
    process::task::{StoppedTask, Task},
    protocol::{Errno, FromTask, ToTask, VString},
};

#[derive(Debug)]
pub struct Exec {
    pub filename: VString,
    pub argv: VStringArray,
    pub envp: VStringArray,
}

impl Exec {
    pub async fn load(self, stopped_task: &mut StoppedTask<'_, '_>) -> Result<(), Errno> {
        let file = ExecFile::new(stopped_task.task, self.filename).await?;
        if script::detect(&file.header) {
            script::load(stopped_task, self, file).await
        } else if elf64::detect(&file.header) {
            elf64::load(stopped_task, self, file).await
        } else {
            Err(Errno(-abi::ENOEXEC))
        }
    }
}

#[derive(Debug)]
#[repr(C)]
#[repr(align(8))]
pub struct FileHeader {
    pub bytes: [u8; abi::BINPRM_BUF_SIZE],
}

impl FileHeader {
    pub fn new(file: &File) -> Result<Self, Errno> {
        let mut header_bytes = [0u8; abi::BINPRM_BUF_SIZE];
        // note that truncation is fine, buffer is zero-filled.
        file.pread(&mut header_bytes, 0)?;
        Ok(FileHeader {
            bytes: header_bytes,
        })
    }
}

#[derive(Debug)]
pub struct ExecFile {
    pub inner: TempFile,
    pub header: FileHeader,
}

impl ExecFile {
    pub async fn new<'q, 's, 't>(task: &'s mut Task<'q>, path: VString) -> Result<Self, Errno> {
        let (_vfile, sysfd) = ipc_call!(
            task,
            FromTask::FileOpen {
                dir: None,
                path,
                flags: abi::O_RDONLY as i32,
                mode: 0,
            },
            ToTask::FileReply(result),
            result?
        );
        let inner = TempFile(File::new(sysfd));
        let header = FileHeader::new(&inner.0)?;
        Ok(ExecFile { inner, header })
    }
}
