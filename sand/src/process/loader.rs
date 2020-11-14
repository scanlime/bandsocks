use crate::{
    abi, binformat,
    binformat::header::Header,
    process::{remote, task::StoppedTask},
    protocol::{Errno, FromTask, SysFd, ToTask, VPtr, VString},
};
use sc::nr;

pub struct Loader<'q, 's, 't> {
    stopped_task: &'t mut StoppedTask<'q, 's>,
    file: SysFd,
    filename: VString,
    argv: VPtr,
    envp: VPtr,
}

impl<'q, 's, 't> Drop for Loader<'q, 's, 't> {
    fn drop(&mut self) {
        self.file.close().unwrap();
    }
}

impl<'q, 's, 't> Loader<'q, 's, 't> {
    pub async fn execve(
        stopped_task: &'t mut StoppedTask<'q, 's>,
        filename: VString,
        argv: VPtr,
        envp: VPtr,
    ) -> Result<(), Errno> {
        Loader::open(stopped_task, filename, argv, envp)
            .await?
            .exec()
            .await
    }

    pub async fn open(
        stopped_task: &'t mut StoppedTask<'q, 's>,
        filename: VString,
        argv: VPtr,
        envp: VPtr,
    ) -> Result<Loader<'q, 's, 't>, Errno> {
        let file = ipc_call!(
            stopped_task.task,
            FromTask::FileOpen {
                dir: None,
                path: filename,
                flags: abi::O_RDONLY as i32,
                mode: 0,
            },
            ToTask::FileReply(result),
            result
        )?;
        Ok(Loader {
            stopped_task,
            file,
            filename,
            argv,
            envp,
        })
    }

    pub async fn exec(self) -> Result<(), Errno> {
        let header = Header::load(&self.file).await?;
        binformat::exec(self, header).await
    }
}
