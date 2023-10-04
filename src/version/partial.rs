use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use regex::Regex;

use super::VersionError;

#[derive(Copy, Clone, Debug)]
pub enum PartialVersion {
    /// Major, Minor, Patch.
    Mmp(u32, u32, u32),
    /// Major, Minor.
    Mm(u32, u32),
    /// Major.
    M(u32),
}

impl PartialVersion {
    /// Provide a sort key that implements [`Ord`].
    ///
    /// `PartialVersion` does not implement [`Eq`] or [`Ord`] because they would
    /// disagree with its [`PartialEq`] and [`PartialOrd`] implementations, so
    /// this function provides a sort key that implements [`Ord`] and can be
    /// used with sorting functions, e.g. [`Vec::sort_by`].
    #[allow(dead_code)]
    pub fn sort_key(&self) -> (u32, Option<u32>, Option<u32>) {
        use PartialVersion::*;
        match *self {
            Mmp(a, b, c) => (a, Some(b), Some(c)),
            Mm(a, b) => (a, Some(b), None),
            M(a) => (a, None, None),
        }
    }
}

impl PartialEq for PartialVersion {
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp(other) == Some(Ordering::Equal)
    }
}

impl PartialOrd for PartialVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use PartialVersion::*;
        match (*self, *other) {
            (Mmp(x1, x2, x3), Mmp(y1, y2, y3)) => (x1, x2, x3).partial_cmp(&(y1, y2, y3)),
            (Mmp(x1, x2, x3), Mm(y1, y2)) => (x1, x2, x3).partial_cmp(&(y1, y2, 0)),
            (Mmp(x1, x2, x3), M(y1)) => (x1, x2, x3).partial_cmp(&(y1, 0, 0)),
            (Mm(x1, x2), Mmp(y1, y2, y3)) => (x1, x2, 0).partial_cmp(&(y1, y2, y3)),
            (Mm(x1, x2), Mm(y1, y2)) => (x1, x2).partial_cmp(&(y1, y2)),
            (Mm(x1, x2), M(y1)) => (x1, x2).partial_cmp(&(y1, 0)),
            (M(x1), Mmp(y1, y2, y3)) => (x1, 0, 0).partial_cmp(&(y1, y2, y3)),
            (M(x1), Mm(y1, y2)) => (x1, 0).partial_cmp(&(y1, y2)),
            (M(x1), M(y1)) => x1.partial_cmp(&y1),
        }
    }
}

impl fmt::Display for PartialVersion {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Mmp(a, b, c) => fmt.pad(&format!("{}.{}.{}", a, b, c)),
            Self::Mm(a, b) => fmt.pad(&format!("{}.{}", a, b)),
            Self::M(a) => fmt.pad(&format!("{}", a)),
        }
    }
}

impl FromStr for PartialVersion {
    type Err = VersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"(?x) \b (\d+) (?: [.] (\d+) (?: [.] (\d+) )? )? \b").unwrap();
        match re.captures(s) {
            Some(caps) => match (
                caps.get(1).and_then(|n| n.as_str().parse::<u32>().ok()),
                caps.get(2).and_then(|n| n.as_str().parse::<u32>().ok()),
                caps.get(3).and_then(|n| n.as_str().parse::<u32>().ok()),
            ) {
                (Some(a), Some(b), Some(c)) => Ok(Self::Mmp(a, b, c)),
                (Some(a), Some(b), _) => Ok(Self::Mm(a, b)),
                (Some(a), _, _) => Ok(Self::M(a)),
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
        assert_eq!(Ok(Mmp(9, 6, 17)), "9.6.17".parse());
    }

    #[test]
    fn parses_version_above_10() {
        assert_eq!(Ok(Mm(12, 2)), "12.2".parse());
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
        assert_eq!("9.6.17", format!("{}", Mmp(9, 6, 17)));
    }

    #[test]
    fn displays_version_above_10() {
        assert_eq!("12.2", format!("{}", Mm(12, 2)));
    }

    #[test]
    fn partial_ord_works_as_expected() {
        let mut versions = vec![
            Mmp(9, 10, 11),
            Mmp(9, 10, 12),
            Mm(8, 11),
            Mm(9, 11),
            Mm(9, 12),
            Mm(10, 11),
            M(8),
            M(9),
            M(11),
        ];
        let mut rng = thread_rng();
        for _ in 0..1000 {
            versions.shuffle(&mut rng);
            versions.sort_by(|a, b| a.partial_cmp(b).unwrap());
            assert_eq!(
                versions,
                vec![
                    M(8),
                    Mm(8, 11),
                    M(9),
                    Mmp(9, 10, 11),
                    Mmp(9, 10, 12),
                    Mm(9, 11),
                    Mm(9, 12),
                    Mm(10, 11),
                    M(11),
                ]
            );
        }
    }

    #[test]
    fn sort_key_works_as_expected() {
        let mut versions = vec![
            Mmp(9, 0, 0),
            Mmp(9, 10, 11),
            Mmp(9, 10, 12),
            Mm(9, 0),
            Mm(8, 11),
            Mm(9, 11),
            Mm(9, 12),
            Mm(10, 11),
            M(8),
            M(9),
            M(11),
        ];
        let mut rng = thread_rng();
        for _ in 0..1000 {
            versions.shuffle(&mut rng);
            versions.sort_by_key(PartialVersion::sort_key);
            assert_eq!(
                versions,
                vec![
                    M(8),
                    Mm(8, 11),
                    M(9),
                    Mm(9, 0),
                    Mmp(9, 0, 0),
                    Mmp(9, 10, 11),
                    Mmp(9, 10, 12),
                    Mm(9, 11),
                    Mm(9, 12),
                    Mm(10, 11),
                    M(11),
                ]
            );
        }
    }
}
