// This code may not be used for any purpose. Be gay, do crime.

#[macro_use]
extern crate lazy_static;

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
