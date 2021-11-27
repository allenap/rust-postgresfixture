use std::str::FromStr;
use std::{error, fmt, io, num};

use regex::Regex;

#[derive(Debug, PartialEq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: Option<u32>,
}

impl fmt::Display for Version {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self.patch {
            Some(patch) => write!(fmt, "{}.{}.{}", self.major, self.minor, patch),
            None => write!(fmt, "{}.{}", self.major, self.minor),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum VersionParseError {
    BadlyFormed,
    Missing,
}

impl fmt::Display for VersionParseError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            VersionParseError::BadlyFormed => write!(fmt, "badly formed"),
            VersionParseError::Missing => write!(fmt, "not found"),
        }
    }
}

impl error::Error for VersionParseError {
    fn cause(&self) -> Option<&dyn error::Error> {
        None
    }
}

impl From<num::ParseIntError> for VersionParseError {
    fn from(_error: num::ParseIntError) -> VersionParseError {
        VersionParseError::BadlyFormed
    }
}

impl FromStr for Version {
    type Err = VersionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"(?x) \b (\d+) [.] (\d+) (?: [.] (\d+) )? \b").unwrap();
        match re.captures(s) {
            Some(caps) => Ok(Version {
                major: caps[1].parse()?,
                minor: caps[2].parse()?,
                patch: match caps.get(3) {
                    Some(m) => Some(m.as_str().parse()?),
                    None => None,
                },
            }),
            None => Err(VersionParseError::Missing),
        }
    }
}

#[derive(Debug)]
pub enum VersionError {
    IoError(io::Error),
    Invalid(VersionParseError),
}

impl fmt::Display for VersionError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            VersionError::IoError(ref e) => write!(fmt, "input/output error: {}", e),
            VersionError::Invalid(ref e) => write!(fmt, "version was invalid: {}", e),
        }
    }
}

impl error::Error for VersionError {
    fn cause(&self) -> Option<&dyn error::Error> {
        match *self {
            VersionError::IoError(ref error) => Some(error),
            VersionError::Invalid(ref error) => Some(error),
        }
    }
}

impl From<io::Error> for VersionError {
    fn from(error: io::Error) -> VersionError {
        VersionError::IoError(error)
    }
}

impl From<VersionParseError> for VersionError {
    fn from(error: VersionParseError) -> VersionError {
        VersionError::Invalid(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{Version, VersionParseError::*};

    #[test]
    fn parses_version_below_10() {
        assert_eq!(Ok(vp(9, 6, 17)), "9.6.17".parse());
    }

    #[test]
    fn parses_version_above_10() {
        assert_eq!(Ok(v(12, 2)), "12.2".parse());
    }

    #[test]
    fn parse_returns_error_when_version_is_invalid() {
        // 4294967295 is (2^32 + 1), so won't fit in a u32.
        assert_eq!(Err(BadlyFormed), "4294967296.0".parse::<Version>());
    }

    #[test]
    fn parse_returns_error_when_version_not_found() {
        assert_eq!(Err(Missing), "foo".parse::<Version>());
    }

    #[test]
    fn displays_version_below_10() {
        assert_eq!("9.6.17", format!("{}", vp(9, 6, 17)));
    }

    #[test]
    fn displays_version_above_10() {
        assert_eq!("12.2", format!("{}", v(12, 2)));
    }

    fn vp(major: u32, minor: u32, patch: u32) -> Version {
        Version {
            major,
            minor,
            patch: Some(patch),
        }
    }

    fn v(major: u32, minor: u32) -> Version {
        Version {
            major,
            minor,
            patch: None,
        }
    }
}
