use std::path::PathBuf;
use std::process::Output;
use std::{error, fmt, io};

use crate::runtime;
use crate::version;

#[derive(Debug)]
pub enum Error {
    PathEncodingError, // Path is not UTF-8.
    IoError(io::Error),
    UnixError(nix::Error),
    UnsupportedVersion(version::Version),
    UnknownVersion(version::Error),
    RuntimeNotFound(version::PartialVersion),
    RuntimeDefaultNotFound,
    DataDirectoryNotFound(PathBuf),
    DatabaseError(postgres::error::Error),
    InUse, // Cluster is already in use; cannot lock exclusively.
    Other(Output),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use Error::*;
        match *self {
            PathEncodingError => write!(fmt, "path is not UTF-8"),
            IoError(ref e) => write!(fmt, "input/output error: {e}"),
            UnixError(ref e) => write!(fmt, "UNIX error: {e}"),
            UnsupportedVersion(ref e) => write!(fmt, "PostgreSQL version not supported: {e}"),
            UnknownVersion(ref e) => write!(fmt, "PostgreSQL version not known: {e}"),
            RuntimeNotFound(ref v) => write!(fmt, "PostgreSQL runtime not found for version {v}"),
            RuntimeDefaultNotFound => write!(fmt, "PostgreSQL runtime not found"),
            DataDirectoryNotFound(ref p) => write!(fmt, "data directory not found in {p:?}"),
            DatabaseError(ref e) => write!(fmt, "database error: {e}"),
            InUse => write!(fmt, "cluster in use; cannot lock exclusively"),
            Other(ref e) => write!(fmt, "external command failed: {e:?}"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            Error::PathEncodingError => None,
            Error::IoError(ref error) => Some(error),
            Error::UnixError(ref error) => Some(error),
            Error::UnsupportedVersion(_) => None,
            Error::UnknownVersion(ref error) => Some(error),
            Error::RuntimeNotFound(_) => None,
            Error::RuntimeDefaultNotFound => None,
            Error::DataDirectoryNotFound(_) => None,
            Error::DatabaseError(ref error) => Some(error),
            Error::InUse => None,
            Error::Other(_) => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::IoError(error)
    }
}

impl From<nix::Error> for Error {
    fn from(error: nix::Error) -> Error {
        Error::UnixError(error)
    }
}

impl From<version::Error> for Error {
    fn from(error: version::Error) -> Error {
        Error::UnknownVersion(error)
    }
}

impl From<postgres::error::Error> for Error {
    fn from(error: postgres::error::Error) -> Error {
        Error::DatabaseError(error)
    }
}

impl From<runtime::Error> for Error {
    fn from(error: runtime::Error) -> Error {
        match error {
            runtime::Error::IoError(error) => Error::IoError(error),
            runtime::Error::VersionError(error) => Error::UnknownVersion(error),
        }
    }
}
