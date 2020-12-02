#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[macro_use] extern crate lazy_static;
#[macro_use] extern crate serde;
#[macro_use] extern crate hash32_derive;

mod container;
mod errors;
mod filesystem;
mod image;
mod ipcserver;
mod manifest;
mod process;
mod registry;
mod sand;
mod taskcall;

pub use crate::{
    container::*,
    errors::*,
    filesystem::{mount::*, socket::*},
    image::*,
    registry::*,
};
