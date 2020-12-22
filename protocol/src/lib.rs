#![no_std]

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[cfg(not(target_arch = "x86_64"))]
compile_error!("bandsocks currently only supports x86_64");

#[macro_use] extern crate serde;

#[cfg(test)]
#[macro_use]
extern crate std;

#[cfg(test)] mod tests;

pub mod abi;
pub mod buffer;
pub mod de;
pub mod ser;

mod messages;
mod types;

pub use messages::*;
pub use types::*;
