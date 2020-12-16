//! Support for downloading container images from a registry server

mod auth;
mod builder;
mod client;
mod default;
mod progress;

pub use builder::RegistryClientBuilder;
pub use client::RegistryClient;
pub use default::DefaultRegistry;
pub use progress::{
    ProgressEvent, ProgressPhase, ProgressResource, ProgressUpdate, Pull, PullProgress,
};
