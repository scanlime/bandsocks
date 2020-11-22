#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[macro_use] extern crate lazy_static;
#[macro_use] extern crate serde;
#[macro_use] extern crate hash32_derive;

pub mod container;
pub mod errors;
pub mod image;
pub mod registry;

mod filesystem;
mod ipcserver;
mod manifest;
mod process;
mod sand;
mod taskcall;

pub use crate::{container::Container, image::ImageName};
