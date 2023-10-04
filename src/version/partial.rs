use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use regex::Regex;

use super::VersionError;

#[derive(Debug)]
pub enum PartialVersion {
    A(u32, u32, u32),
    B(u32, u32),
    C(u32),
}

impl PartialEq for PartialVersion {
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp(other) == Some(Ordering::Equal)
    }
}

impl Eq for PartialVersion {}

impl PartialOrd for PartialVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use PartialVersion::*;
        match (self, other) {
            (A(x1, x2, x3), A(y1, y2, y3)) => (x1, x2, x3).partial_cmp(&(y1, y2, y3)),
            (A(x1, x2, x3), B(y1, y2)) => (x1, x2, x3).partial_cmp(&(y1, y2, &0)),
            (A(x1, x2, x3), C(y1)) => (x1, x2, x3).partial_cmp(&(y1, &0, &0)),
            (B(x1, x2), A(y1, y2, y3)) => (x1, x2, &0).partial_cmp(&(y1, y2, y3)),
            (B(x1, x2), B(y1, y2)) => (x1, x2).partial_cmp(&(y1, y2)),
            (B(x1, x2), C(y1)) => (x1, x2).partial_cmp(&(y1, &0)),
            (C(x1), A(y1, y2, y3)) => (x1, &0, &0).partial_cmp(&(y1, y2, y3)),
            (C(x1), B(y1, y2)) => (x1, &0).partial_cmp(&(y1, y2)),
            (C(x1), C(y1)) => x1.partial_cmp(y1),
        }
    }
}

impl Ord for PartialVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use PartialVersion::*;
        match (self, other) {
            (A(x1, x2, x3), A(y1, y2, y3)) => (x1, x2, x3).cmp(&(y1, y2, y3)),
            (A(x1, x2, x3), B(y1, y2)) => (x1, x2, x3).cmp(&(y1, y2, &0)),
            (A(x1, x2, x3), C(y1)) => (x1, x2, x3).cmp(&(y1, &0, &0)),
            (B(x1, x2), A(y1, y2, y3)) => (x1, x2, &0).cmp(&(y1, y2, y3)),
            (B(x1, x2), B(y1, y2)) => (x1, x2).cmp(&(y1, y2)),
            (B(x1, x2), C(y1)) => (x1, x2).cmp(&(y1, &0)),
            (C(x1), A(y1, y2, y3)) => (x1, &0, &0).cmp(&(y1, y2, y3)),
            (C(x1), B(y1, y2)) => (x1, &0).cmp(&(y1, y2)),
            (C(x1), C(y1)) => x1.cmp(y1),
        }
    }
}

impl fmt::Display for PartialVersion {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::A(a, b, c) => fmt.pad(&format!("{}.{}.{}", a, b, c)),
            Self::B(a, b) => fmt.pad(&format!("{}.{}", a, b)),
            Self::C(a) => fmt.pad(&format!("{}", a)),
        }
    }
}

impl FromStr for PartialVersion {
    type Err = VersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"(?x) \b (\d+) (?: [.] (\d+) )? (?: [.] (\d+) )? \b").unwrap();
        match re.captures(s) {
            Some(caps) => match (
                caps.get(1).and_then(|n| n.as_str().parse::<u32>().ok()),
                caps.get(2).and_then(|n| n.as_str().parse::<u32>().ok()),
                caps.get(3).and_then(|n| n.as_str().parse::<u32>().ok()),
            ) {
                (Some(a), Some(b), Some(c)) => Ok(Self::A(a, b, c)),
                (Some(a), Some(b), _) => Ok(Self::B(a, b)),
                (Some(a), _, _) => Ok(Self::C(a)),
                _ => Err(VersionError::BadlyFormed),
            },
            None => Err(VersionError::Missing),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::VersionError::*;
    use super::{PartialVersion, PartialVersion::*};

    use rand::seq::SliceRandom;
    use rand::thread_rng;

    #[test]
    fn parses_version_below_10() {
        assert_eq!(Ok(A(9, 6, 17)), "9.6.17".parse());
    }

    #[test]
    fn parses_version_above_10() {
        assert_eq!(Ok(B(12, 2)), "12.2".parse());
    }

    #[test]
    fn parse_returns_error_when_version_is_invalid() {
        // 4294967295 is (2^32 + 1), so won't fit in a u32.
        assert_eq!(Err(BadlyFormed), "4294967296.0".parse::<PartialVersion>());
    }

    #[test]
    fn parse_returns_error_when_version_not_found() {
        assert_eq!(Err(Missing), "foo".parse::<PartialVersion>());
    }

    #[test]
    fn displays_version_below_10() {
        assert_eq!("9.6.17", format!("{}", A(9, 6, 17)));
    }

    #[test]
    fn displays_version_above_10() {
        assert_eq!("12.2", format!("{}", B(12, 2)));
    }

    #[test]
    fn derive_partial_ord_works_as_expected() {
        let mut versions = vec![
            A(9, 10, 11),
            A(9, 10, 12),
            B(8, 11),
            B(9, 11),
            B(9, 12),
            B(10, 11),
            C(8),
            C(9),
            C(11),
        ];
        let mut rng = thread_rng();
        for _ in 0..1000 {
            versions.shuffle(&mut rng);
            versions.sort_by(|a, b| a.partial_cmp(b).unwrap());
            assert_eq!(
                versions,
                vec![
                    C(8),
                    B(8, 11),
                    C(9),
                    A(9, 10, 11),
                    A(9, 10, 12),
                    B(9, 11),
                    B(9, 12),
                    B(10, 11),
                    C(11),
                ]
            );
        }
    }

    #[test]
    fn derive_ord_works_as_expected() {
        let mut versions = vec![
            A(9, 10, 11),
            A(9, 10, 12),
            B(8, 11),
            B(9, 11),
            B(9, 12),
            B(10, 11),
            C(8),
            C(9),
            C(11),
        ];
        let mut rng = thread_rng();
        for _ in 0..1000 {
            versions.shuffle(&mut rng);
            versions.sort();
            assert_eq!(
                versions,
                vec![
                    C(8),
                    B(8, 11),
                    C(9),
                    A(9, 10, 11),
                    A(9, 10, 12),
                    B(9, 11),
                    B(9, 12),
                    B(10, 11),
                    C(11),
                ]
            );
        }
    }
}
