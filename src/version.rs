//! Parse PostgreSQL version numbers.
//!
//! ```rust
//! # use postgresfixture::version::Version;
//! assert_eq!(Ok(Version::Pre10(9, 6, 17)), "9.6.17".parse());
//! assert_eq!(Ok(Version::Post10(14, 6)), "14.6".parse());
//! ```
//!
//! See <https://www.postgresql.org/support/versioning/> for information on
//! PostgreSQL's versioning scheme.

mod partial;

use std::str::FromStr;
use std::{error, fmt, num};

use regex::Regex;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Version {
    Pre10(u32, u32, u32),
    Post10(u32, u32),
}

impl fmt::Display for Version {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Version::Pre10(a, b, c) => fmt.pad(&format!("{}.{}.{}", a, b, c)),
            Version::Post10(a, b) => fmt.pad(&format!("{}.{}", a, b)),
        }
    }
}

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

impl FromStr for Version {
    type Err = VersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"(?x) \b (\d+) [.] (\d+) (?: [.] (\d+) )? \b").unwrap();
        match re.captures(s) {
            Some(caps) => {
                let a = caps[1].parse::<u32>()?;
                let b = caps[2].parse::<u32>()?;
                match caps.get(3) {
                    Some(m) => {
                        let c = m.as_str().parse::<u32>()?;
                        if a >= 10 {
                            Err(VersionError::BadlyFormed)
                        } else {
                            Ok(Version::Pre10(a, b, c))
                        }
                    }
                    None => {
                        if a < 10 {
                            Err(VersionError::BadlyFormed)
                        } else {
                            Ok(Version::Post10(a, b))
                        }
                    }
                }
            }
            None => Err(VersionError::Missing),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Version::{Post10, Pre10};
    use super::{Version, VersionError::*};

    use std::cmp::Ordering;

    #[test]
    fn parses_version_below_10() {
        assert_eq!(Ok(Pre10(9, 6, 17)), "9.6.17".parse());
    }

    #[test]
    fn parses_version_above_10() {
        assert_eq!(Ok(Post10(12, 2)), "12.2".parse());
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
        assert_eq!("9.6.17", format!("{}", Pre10(9, 6, 17)));
    }

    #[test]
    fn displays_version_above_10() {
        assert_eq!("12.2", format!("{}", Post10(12, 2)));
    }

    #[test]
        #[rustfmt::skip]
        fn derive_partial_ord_works_as_expected() {
            assert_eq!(Pre10(9, 10, 11).partial_cmp(&Post10(10, 11)), Some(Ordering::Less));
            assert_eq!(Post10(10, 11).partial_cmp(&Pre10(9, 10, 11)), Some(Ordering::Greater));
            assert_eq!(Pre10(9, 10, 11).partial_cmp(&Pre10(9, 10, 11)), Some(Ordering::Equal));
            assert_eq!(Post10(10, 11).partial_cmp(&Post10(10, 11)), Some(Ordering::Equal));
        }

    #[test]
    fn derive_ord_works_as_expected() {
        let mut versions = vec![
            Pre10(9, 10, 11),
            Post10(10, 11),
            Post10(14, 2),
            Pre10(9, 10, 12),
            Post10(10, 12),
        ];
        versions.sort(); // Uses `Ord`.
        assert_eq!(
            versions,
            vec![
                Pre10(9, 10, 11),
                Pre10(9, 10, 12),
                Post10(10, 11),
                Post10(10, 12),
                Post10(14, 2)
            ]
        );
    }
}
