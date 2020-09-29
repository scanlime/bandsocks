// This code may not be used for any purpose. Be gay, do crime.

use thiserror::Error;
use std::fmt;

#[derive(Error, Debug)]
pub enum ImageError {
    Registry {
        #[from]
        source: dkregistry::errors::Error
    }
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::Registry { source } => write!(f, "registry error: {}", source),
        }
    }
}
