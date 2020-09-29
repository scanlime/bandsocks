// This code may not be used for any purpose. Be gay, do crime.

#[cfg(any(target_os="android", target_os="linux"))]
mod linux;

mod errors;
mod image;

pub use dkregistry::reference::Reference;
pub use image::{Image, Cache, CacheBuilder};
pub use errors::{ImageError};
