use std::path::PathBuf;
use std::process::Output;
use std::{error, fmt, io};

use crate::runtime;
use crate::version;

#[derive(Debug)]
pub enum ClusterError {
    PathEncodingError, // Path is not UTF-8.
    IoError(io::Error),
    UnixError(nix::Error),
    UnsupportedVersion(version::Version),
    UnknownVersion(version::VersionError),
    RuntimeNotFound(version::PartialVersion),
    RuntimeDefaultNotFound,
    DataDirectoryNotFound(PathBuf),
    DatabaseError(postgres::error::Error),
    InUse, // Cluster is already in use; cannot lock exclusively.
    Other(Output),
}

impl fmt::Display for ClusterError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use ClusterError::*;
        match *self {
            PathEncodingError => write!(fmt, "path is not UTF-8"),
            IoError(ref e) => write!(fmt, "input/output error: {}", e),
            UnixError(ref e) => write!(fmt, "UNIX error: {}", e),
            UnsupportedVersion(ref e) => write!(fmt, "PostgreSQL version not supported: {}", e),
            UnknownVersion(ref e) => write!(fmt, "PostgreSQL version not known: {}", e),
            RuntimeNotFound(ref v) => write!(fmt, "PostgreSQL runtime not found for version {v}"),
            RuntimeDefaultNotFound => write!(fmt, "PostgreSQL runtime not found"),
            DataDirectoryNotFound(ref p) => write!(fmt, "data directory not found in {p:?}"),
            DatabaseError(ref e) => write!(fmt, "database error: {}", e),
            InUse => write!(fmt, "cluster in use; cannot lock exclusively"),
            Other(ref e) => write!(fmt, "external command failed: {:?}", e),
        }
    }
}

impl error::Error for ClusterError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            ClusterError::PathEncodingError => None,
            ClusterError::IoError(ref error) => Some(error),
            ClusterError::UnixError(ref error) => Some(error),
            ClusterError::UnsupportedVersion(_) => None,
            ClusterError::UnknownVersion(ref error) => Some(error),
            ClusterError::RuntimeNotFound(_) => None,
            ClusterError::RuntimeDefaultNotFound => None,
            ClusterError::DataDirectoryNotFound(_) => None,
            ClusterError::DatabaseError(ref error) => Some(error),
            ClusterError::InUse => None,
            ClusterError::Other(_) => None,
        }
    }
}

impl From<io::Error> for ClusterError {
    fn from(error: io::Error) -> ClusterError {
        ClusterError::IoError(error)
    }
}

impl From<nix::Error> for ClusterError {
    fn from(error: nix::Error) -> ClusterError {
        ClusterError::UnixError(error)
    }
}

impl From<version::VersionError> for ClusterError {
    fn from(error: version::VersionError) -> ClusterError {
        ClusterError::UnknownVersion(error)
    }
}

impl From<postgres::error::Error> for ClusterError {
    fn from(error: postgres::error::Error) -> ClusterError {
        ClusterError::DatabaseError(error)
    }
}

impl From<runtime::RuntimeError> for ClusterError {
    fn from(error: runtime::RuntimeError) -> ClusterError {
        match error {
            runtime::RuntimeError::IoError(error) => ClusterError::IoError(error),
            runtime::RuntimeError::UnknownVersion(error) => ClusterError::UnknownVersion(error),
        }
    }
}
