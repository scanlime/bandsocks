#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[macro_use] extern crate lazy_static;
#[macro_use] extern crate serde;
#[macro_use] extern crate hash32_derive;

mod client;
mod container;
mod errors;
mod filesystem;
mod image;
mod ipcserver;
mod manifest;
mod sand;
mod storage;

pub use crate::client::Client;
pub use crate::container::Container;
pub use crate::errors::ImageError;
pub use crate::image::Image;
pub use dkregistry::reference::Reference;
