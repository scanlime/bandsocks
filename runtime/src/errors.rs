// This code may not be used for any purpose. Be gay, do crime.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ImageError {

    #[error("registry error: {}", source)]
    Registry {
        #[from]
        source: dkregistry::errors::Error
    },

    #[error("can't determine where to cache image files")]
    NoDefaultCacheDir,
}
