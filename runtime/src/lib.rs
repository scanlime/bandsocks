// This code may not be used for any purpose. Be gay, do crime.

#[macro_use]
extern crate lazy_static;

#[cfg(any(target_os="android", target_os="linux"))]
mod linux;

mod container;
mod client;
mod errors;
mod filesystem;
mod image;
mod manifest;
mod storage;

pub use dkregistry::reference::Reference;
pub use crate::container::Container;
pub use crate::image::Image;
pub use crate::client::Client;
pub use crate::errors::ImageError;
