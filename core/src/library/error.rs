//! Errors for the library module.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("tag: {0}")]
    Tag(String),
    #[error("watch: {0}")]
    Watch(String),
}
