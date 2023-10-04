use std::{error, fmt, num};

/// Error parsing a PostgreSQL version number.
#[derive(Debug, PartialEq)]
pub enum VersionError {
    BadlyFormed,
    Missing,
}

impl fmt::Display for VersionError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            VersionError::BadlyFormed => write!(fmt, "badly formed"),
            VersionError::Missing => write!(fmt, "not found"),
        }
    }
}

impl error::Error for VersionError {
    fn cause(&self) -> Option<&dyn error::Error> {
        None
    }
}

impl From<num::ParseIntError> for VersionError {
    fn from(_error: num::ParseIntError) -> VersionError {
        VersionError::BadlyFormed
    }
}
