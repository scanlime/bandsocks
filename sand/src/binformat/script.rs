use crate::{
    binformat::{Exec, ExecFile, FileHeader},
    process::task::StoppedTask,
    protocol::Errno,
};

pub fn detect(_header: &FileHeader) -> bool {
    false
}

pub async fn load<'q, 's, 't>(
    _stopped_task: &'t mut StoppedTask<'q, 's>,
    _exec: Exec,
    _file: ExecFile,
) -> Result<(), Errno> {
    Ok(())
}
