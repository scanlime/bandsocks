//! Progress updates for interactions with an image registry

use crate::{
    errors::ImageError,
    image::{ContentDigest, Image, ImageVersion, Registry, Repository},
};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Channel for recieving progress information for an image pull
///
/// This is a stream of progress updates, culminating in a complete [Image] or
/// an error. Created by [crate::RegistryClient::pull_progress()]
pub struct Pull {
    pub(crate) receiver: mpsc::Receiver<PullProgress>,
}

impl Pull {
    /// Wait for an ongoing image pull operation to complete
    pub async fn wait(self) -> Result<Arc<Image>, ImageError> {
        let mut pull = self;
        loop {
            match pull.progress().await {
                PullProgress::Update(_) => (),
                PullProgress::Done(result) => return result,
            }
        }
    }

    /// Wait for the image pull to make some progress
    pub async fn progress(&mut self) -> PullProgress {
        match self.receiver.recv().await {
            Some(progress) => progress,
            None => PullProgress::Done(Err(ImageError::PullTaskError)),
        }
    }
}

/// Progress for an image pull
///
/// [Pull::progress()] returns this, indicating either completion or a
/// [ProgressUpdate]
#[derive(Debug)]
pub enum PullProgress {
    Done(Result<Arc<Image>, ImageError>),
    Update(ProgressUpdate),
}

/// An update on the state of an asynchronous registry operation
///
/// Each operation pertains to one [ProgressResource], and is additionally
/// described by a [ProgressPhase] and a [ProgressEvent].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressUpdate {
    pub resource: Arc<ProgressResource>,
    pub phase: ProgressPhase,
    pub event: ProgressEvent,
}

/// Which resource on the registry does this progress update pertain to
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProgressResource {
    Blob(ContentDigest),
    Manifest(Registry, Repository, ImageVersion),
}

/// What operational phase are we reporting on, within the particular resource
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProgressPhase {
    Connect,
    Download,
    Decompress,
}

/// An amount of progress toward one operation in an asynchronous registry
/// operation
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProgressEvent {
    Begin,
    BeginSized(u64),
    Progress(u64),
    Complete,
}
