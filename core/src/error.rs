//! The crate's error type: a readable message, with the file it concerns when there is one.

use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn new(msg: impl Into<String>) -> Error {
        Error(msg.into())
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error(e.to_string())
    }
}

impl From<String> for Error {
    fn from(s: String) -> Error {
        Error(s)
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Error {
        Error(s.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error(e.to_string())
    }
}

/// Prefix an error with what was being done or which file it was.
pub trait Context<T> {
    fn context(self, what: impl fmt::Display) -> Result<T>;
    fn path_context(self, path: &Path) -> Result<T>;
}

impl<T, E: Into<Error>> Context<T> for std::result::Result<T, E> {
    fn context(self, what: impl fmt::Display) -> Result<T> {
        self.map_err(|e| Error(format!("{what}: {}", e.into().0)))
    }

    fn path_context(self, path: &Path) -> Result<T> {
        self.context(path.display())
    }
}
