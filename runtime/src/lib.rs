// This code may not be used for any purpose. Be gay, do crime.

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[macro_use]
extern crate lazy_static;

mod container;
mod client;
mod errors;
mod filesystem;
mod image;
mod manifest;
mod sand;
mod storage;

pub use dkregistry::reference::Reference;
pub use crate::container::Container;
pub use crate::image::Image;
pub use crate::client::Client;
pub use crate::errors::ImageError;
