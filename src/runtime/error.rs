use std::{error, fmt, io};

use crate::version;

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    VersionError(version::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use Error::*;
        match *self {
            IoError(ref e) => write!(fmt, "input/output error: {e}"),
            VersionError(ref e) => e.fmt(fmt),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            Error::IoError(ref error) => Some(error),
            Error::VersionError(ref error) => Some(error),
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::IoError(error)
    }
}

impl From<version::Error> for Error {
    fn from(error: version::Error) -> Error {
        Error::VersionError(error)
    }
}
