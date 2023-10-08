use std::{error, fmt, num};

/// Error parsing a PostgreSQL version number.
#[derive(Debug, PartialEq)]
pub enum Error {
    BadlyFormed,
    Missing,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::BadlyFormed => write!(fmt, "badly formed"),
            Error::Missing => write!(fmt, "not found"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}

impl From<num::ParseIntError> for Error {
    fn from(_error: num::ParseIntError) -> Error {
        Error::BadlyFormed
    }
}
