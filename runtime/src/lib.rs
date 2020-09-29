// This code may not be used for any purpose. Be gay, do crime.

#[cfg(any(target_os="android", target_os="linux"))]
mod linux;

mod errors;
mod client;
mod storage;
mod image;

pub use dkregistry::reference::Reference;

pub use crate::image::Image;
pub use crate::client::Client;
pub use crate::errors::ImageError;
