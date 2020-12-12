#![no_std]

#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("bandsocks only works on linux or android");

#[cfg(not(target_arch = "x86_64"))]
compile_error!("bandsocks currently only supports x86_64");

#[macro_use] extern crate serde;
#[macro_use] extern crate hash32_derive;

#[cfg(test)]
#[macro_use]
extern crate std;

#[cfg(test)]
mod tests;

pub mod abi;
pub mod buffer;
pub mod de;
pub mod ser;

mod types;
mod messages;

pub use types::*;
pub use messages::*;
