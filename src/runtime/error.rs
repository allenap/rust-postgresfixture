use std::{error, fmt, io};

use crate::version;

#[derive(Debug)]
pub enum RuntimeError {
    IoError(io::Error),
    VersionError(version::VersionError),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use RuntimeError::*;
        match *self {
            IoError(ref e) => write!(fmt, "input/output error: {e}"),
            VersionError(ref e) => e.fmt(fmt),
        }
    }
}

impl error::Error for RuntimeError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            RuntimeError::IoError(ref error) => Some(error),
            RuntimeError::VersionError(ref error) => Some(error),
        }
    }
}

impl From<io::Error> for RuntimeError {
    fn from(error: io::Error) -> RuntimeError {
        RuntimeError::IoError(error)
    }
}

impl From<version::VersionError> for RuntimeError {
    fn from(error: version::VersionError) -> RuntimeError {
        RuntimeError::VersionError(error)
    }
}
