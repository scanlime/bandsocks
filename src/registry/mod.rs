//! Support for downloading container images from a registry server

mod auth;
mod client;
mod default;

pub use client::*;
pub use default::*;
