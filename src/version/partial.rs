use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use regex::Regex;

use super::{Version, VersionError};

/// Represents a PostgreSQL version with some parts missing. This is the kind of
/// thing we might find in a cluster's `PG_VERSION` file.
#[derive(Copy, Clone, Debug)]
pub enum PartialVersion {
    /// Pre-PostgreSQL 10, with major and minor version numbers, e.g. 9.6. It is
    /// an error to create this variant with a major number >= 10; see
    /// [`checked`][`Self::checked`] for a way to guard against this.
    Pre10m(u32, u32),
    /// Pre-PostgreSQL 10, with major, minor, and patch version numbers, e.g.
    /// 9.6.17. It is an error to create this variant with a major number >= 10;
    /// see [`checked`][`Self::checked`] for a way to guard against this.
    Pre10mm(u32, u32, u32),
    /// PostgreSQL 10+, with major version number, e.g. 10. It is an error to
    /// create this variant with a major number < 10; see
    /// [`checked`][`Self::checked`] for a way to guard against this.
    Post10m(u32),
    /// PostgreSQL 10+, with major and minor version number, e.g. 10.3. It is an
    /// error to create this variant with a major number < 10; see
    /// [`checked`][`Self::checked`] for a way to guard against this.
    Post10mm(u32, u32),
}

/// Convert a [`PartialVersion`] into a [`Version`] that's useful for
/// comparisons.
///
/// The [`Version`] returned has 0 (zero) in the place of the missing parts. For
/// example, a partial version of `9.6.*` becomes `9.6.0`, and `12.*` becomes
/// `12.0`.
impl From<&PartialVersion> for Version {
    fn from(partial: &PartialVersion) -> Self {
        use PartialVersion::*;
        match *partial {
            Pre10m(a, b) => Version::Pre10(a, b, 0),
            Pre10mm(a, b, c) => Version::Pre10(a, b, c),
            Post10m(a) => Version::Post10(a, 0),
            Post10mm(a, b) => Version::Post10(a, b),
        }
    }
}

/// See `From<&PartialVersion> for Version`.
impl From<PartialVersion> for Version {
    fn from(partial: PartialVersion) -> Self {
        (&partial).into()
    }
}

/// Convert a [`Version`] into a [`PartialVersion`].
impl From<&Version> for PartialVersion {
    fn from(version: &Version) -> Self {
        use Version::*;
        match *version {
            Pre10(a, b, c) => PartialVersion::Pre10mm(a, b, c),
            Post10(a, b) => PartialVersion::Post10mm(a, b),
        }
    }
}

/// See `From<&Version> for PartialVersion`.
impl From<Version> for PartialVersion {
    fn from(version: Version) -> Self {
        (&version).into()
    }
}

impl PartialVersion {
    /// Return self if it is a valid [`PartialVersion`].
    ///
    /// This can be necessary when a [`PartialVersion`] has been constructed
    /// directly. It checks that [`PartialVersion::Pre10m`] and
    /// [`PartialVersion::Pre10mm`] have a major version number less than 10,
    /// and that [`PartialVersion::Post10m`] and [`PartialVersion::Post10mm`]
    /// have a major version number greater than or equal to 10.
    pub fn checked(self) -> Result<Self, VersionError> {
        use PartialVersion::*;
        match self {
            Pre10m(a, ..) | Pre10mm(a, ..) if a < 10 => Ok(self),
            Post10m(a) | Post10mm(a, ..) if a >= 10 => Ok(self),
            _ => Err(VersionError::BadlyFormed),
        }
    }

    /// Is the given [`Version`] compatible with this [`PartialVersion`]?
    ///
    /// Put another way: can a server of the given [`Version`] be used to run a
    /// cluster of this [`PartialVersion`]?
    ///
    /// This is an interesting question to answer because clusters contain a
    /// file named `PG_VERSION` which containing just the major version
    /// number/numbers of the cluster's files, e.g. "15" or "9.6".
    ///
    /// For versions of PostgreSQL before 10, this means that the given
    /// version's major numbers must match exactly, and the patch number must be
    /// greater than or equal to this `PartialVersion`'s patch number. When this
    /// `PartialVersion` has no minor or patch number, the given version is
    /// assumed to be compatible.
    ///
    /// For versions of PostgreSQL after and including 10, this means that the
    /// given version's major number must match exactly, and the minor number
    /// must be greater than or equal to this `PartialVersion`'s minor number.
    /// When this `PartialVersion` has no minor number, the given version is
    /// assumed to be compatible.
    #[allow(dead_code)]
    pub fn compatible(&self, version: Version) -> bool {
        use PartialVersion::*;
        match (*self, version) {
            (Pre10m(a, b), Version::Pre10(x, y, _)) => a == x && b == y,
            (Pre10mm(a, b, c), Version::Pre10(x, y, z)) => a == x && b == y && c <= z,
            (Post10m(a), Version::Post10(x, _)) => a == x,
            (Post10mm(a, b), Version::Post10(x, y)) => a == x && b <= y,
            _ => false,
        }
    }

    /// Remove minor/patch number.
    pub fn widened(&self) -> PartialVersion {
        use PartialVersion::*;
        match self {
            Pre10mm(a, b, _) => Pre10m(*a, *b),
            Post10mm(a, _) => Post10m(*a),
            _ => *self,
        }
    }

    /// Provide a sort key that implements [`Ord`].
    ///
    /// `PartialVersion` does not implement [`Eq`] or [`Ord`] because they would
    /// disagree with its [`PartialEq`] and [`PartialOrd`] implementations, so
    /// this function provides a sort key that implements [`Ord`] and can be
    /// used with sorting functions, e.g. [`slice::sort_by_key`].
    #[allow(dead_code)]
    pub fn sort_key(&self) -> (u32, Option<u32>, Option<u32>) {
        use PartialVersion::*;
        match *self {
            Pre10m(a, b) => (a, Some(b), None),
            Pre10mm(a, b, c) => (a, Some(b), Some(c)),
            Post10m(a) => (a, None, None),
            Post10mm(a, b) => (a, Some(b), None),
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
            (Pre10m(a, b), Pre10m(x, y)) => Some((a, b).cmp(&(x, y))),
            (Pre10m(a, b), Pre10mm(x, y, _)) => Some((a, b).cmp(&(x, y))),
            (Pre10mm(a, b, _), Pre10m(x, y)) => Some((a, b).cmp(&(x, y))),
            (Pre10mm(a, b, c), Pre10mm(x, y, z)) => Some((a, b, c).cmp(&(x, y, z))),

            (Post10m(a), Post10m(x)) => Some(a.cmp(&x)),
            (Post10m(a), Post10mm(x, _)) => Some(a.cmp(&x)),
            (Post10mm(a, _), Post10m(x)) => Some(a.cmp(&x)),
            (Post10mm(a, b), Post10mm(x, y)) => Some((a, b).cmp(&(x, y))),

            (Pre10m(..), Post10m(..)) => Some(Ordering::Less),
            (Pre10m(..), Post10mm(..)) => Some(Ordering::Less),
            (Pre10mm(..), Post10m(..)) => Some(Ordering::Less),
            (Pre10mm(..), Post10mm(..)) => Some(Ordering::Less),

            (Post10m(..), Pre10m(..)) => Some(Ordering::Greater),
            (Post10m(..), Pre10mm(..)) => Some(Ordering::Greater),
            (Post10mm(..), Pre10m(..)) => Some(Ordering::Greater),
            (Post10mm(..), Pre10mm(..)) => Some(Ordering::Greater),
        }
    }
}

impl fmt::Display for PartialVersion {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Pre10m(a, b) => fmt.pad(&format!("{a}.{b}")),
            Self::Pre10mm(a, b, c) => fmt.pad(&format!("{a}.{b}.{c}")),
            Self::Post10m(a) => fmt.pad(&format!("{a}")),
            Self::Post10mm(a, b) => fmt.pad(&format!("{a}.{b}")),
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
                (Some(a), Some(b), None) if a < 10 => Ok(Self::Pre10m(a, b)),
                (Some(a), Some(b), Some(c)) if a < 10 => Ok(Self::Pre10mm(a, b, c)),
                (Some(a), None, None) if a >= 10 => Ok(Self::Post10m(a)),
                (Some(a), Some(b), None) if a >= 10 => Ok(Self::Post10mm(a, b)),
                _ => Err(VersionError::BadlyFormed),
            },
            None => Err(VersionError::Missing),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Version, VersionError::*};
    use super::{PartialVersion, PartialVersion::*};

    use rand::seq::SliceRandom;
    use rand::thread_rng;

    #[test]
    fn parses_version_below_10() {
        assert_eq!(Ok(Pre10mm(9, 6, 17)), "9.6.17".parse());
        assert_eq!(Ok(Pre10m(9, 6)), "9.6".parse());
    }

    #[test]
    fn parses_version_above_10() {
        assert_eq!(Ok(Post10mm(12, 2)), "12.2".parse());
        assert_eq!(Ok(Post10m(12)), "12".parse());
    }

    #[test]
    fn parse_returns_error_when_version_is_invalid() {
        // 4294967295 is (2^32 + 1), so won't fit in a u32.
        assert_eq!(Err(BadlyFormed), "4294967296.0".parse::<PartialVersion>());
        // Before version 10, there are always at least two parts in a version.
        assert_eq!(Err(BadlyFormed), "9".parse::<PartialVersion>());
        // From version 10 onwards, there are only two parts in a version.
        assert_eq!(Err(BadlyFormed), "10.10.10".parse::<PartialVersion>());
    }

    #[test]
    fn parse_returns_error_when_version_not_found() {
        assert_eq!(Err(Missing), "foo".parse::<PartialVersion>());
    }

    #[test]
    fn checked_returns_self_when_variant_is_valid() {
        use PartialVersion::*;
        assert_eq!(Ok(Pre10m(9, 0)), Pre10m(9, 0).checked());
        assert_eq!(Ok(Pre10mm(9, 0, 0)), Pre10mm(9, 0, 0).checked());
        assert_eq!(Ok(Post10m(10)), Post10m(10).checked());
        assert_eq!(Ok(Post10mm(10, 0)), Post10mm(10, 0).checked());
    }

    #[test]
    fn checked_returns_error_when_variant_is_invalid() {
        use PartialVersion::*;
        assert_eq!(Err(BadlyFormed), Pre10m(10, 0).checked());
        assert_eq!(Err(BadlyFormed), Pre10mm(10, 0, 0).checked());
        assert_eq!(Err(BadlyFormed), Post10m(9).checked());
        assert_eq!(Err(BadlyFormed), Post10mm(9, 0).checked());
    }

    #[test]
    fn displays_version_below_10() {
        assert_eq!("9.6.17", format!("{}", Pre10mm(9, 6, 17)));
        assert_eq!("9.6", format!("{}", Pre10m(9, 6)));
    }

    #[test]
    fn displays_version_above_10() {
        assert_eq!("12.2", format!("{}", Post10mm(12, 2)));
        assert_eq!("12", format!("{}", Post10m(12)));
    }

    #[test]
    fn converts_partial_version_to_version() {
        assert_eq!(Version::Pre10(9, 1, 2), Pre10mm(9, 1, 2).into());
        assert_eq!(Version::Pre10(9, 1, 0), Pre10m(9, 1).into());
        assert_eq!(Version::Post10(14, 2), Post10mm(14, 2).into());
        assert_eq!(Version::Post10(14, 0), Post10m(14).into());
    }

    #[test]
    fn compatible_below_10() {
        let version = "9.6.16".parse().unwrap();
        assert!(Pre10mm(9, 6, 16).compatible(version));
        assert!(Pre10m(9, 6).compatible(version));
    }

    #[test]
    fn not_compatible_below_10() {
        let version = "9.6.16".parse().unwrap();
        assert!(!Pre10mm(9, 6, 17).compatible(version));
        assert!(!Pre10m(9, 7).compatible(version));
        assert!(!Pre10mm(8, 6, 16).compatible(version));
        assert!(!Pre10m(8, 6).compatible(version));
    }

    #[test]
    fn compatible_above_10() {
        let version = "12.6".parse().unwrap();
        assert!(Post10mm(12, 6).compatible(version));
        assert!(Post10m(12).compatible(version));
    }

    #[test]
    fn not_compatible_above_10() {
        let version = "12.6".parse().unwrap();
        assert!(!Post10mm(12, 7).compatible(version));
        assert!(!Post10m(13).compatible(version));
        assert!(!Post10mm(11, 6).compatible(version));
        assert!(!Post10m(11).compatible(version));
    }

    #[test]
    fn not_compatible_below_10_with_above_10() {
        let version = "12.6".parse().unwrap();
        assert!(!Pre10m(9, 1).compatible(version));
        assert!(!Pre10mm(9, 1, 2).compatible(version));
        let version = "9.1.2".parse().unwrap();
        assert!(!Post10m(12).compatible(version));
        assert!(!Post10mm(12, 6).compatible(version));
    }

    #[test]
    fn widened_removes_minor_or_patch_number() {
        assert_eq!(Pre10mm(9, 1, 2), Pre10m(9, 1));
        assert_eq!(Post10mm(12, 9), Post10m(12));
        assert_eq!(Pre10m(9, 1), Pre10m(9, 1));
        assert_eq!(Post10m(12), Post10m(12));
    }

    #[test]
    fn partial_ord_works_as_expected() {
        let mut versions = vec![
            Pre10mm(9, 10, 11),
            Pre10mm(9, 10, 12),
            Pre10m(8, 11),
            Pre10m(9, 11),
            Pre10m(9, 12),
            Post10mm(10, 11),
            Post10m(11),
        ];
        let mut rng = thread_rng();
        for _ in 0..1000 {
            versions.shuffle(&mut rng);
            versions.sort_by(|a, b| a.partial_cmp(b).unwrap());
            assert_eq!(
                versions,
                vec![
                    Pre10m(8, 11),
                    Pre10mm(9, 10, 11),
                    Pre10mm(9, 10, 12),
                    Pre10m(9, 11),
                    Pre10m(9, 12),
                    Post10mm(10, 11),
                    Post10m(11),
                ]
            );
        }
    }

    #[test]
    fn sort_key_works_as_expected() {
        let mut versions = vec![
            Pre10mm(9, 0, 0),
            Pre10mm(9, 10, 11),
            Pre10mm(9, 10, 12),
            Pre10m(9, 0),
            Pre10m(8, 11),
            Pre10m(9, 11),
            Pre10m(9, 12),
            Post10mm(10, 11),
            Post10m(11),
        ];
        let mut rng = thread_rng();
        for _ in 0..1000 {
            versions.shuffle(&mut rng);
            versions.sort_by_key(PartialVersion::sort_key);
            assert_eq!(
                versions,
                vec![
                    Pre10m(8, 11),
                    Pre10m(9, 0),
                    Pre10mm(9, 0, 0),
                    Pre10mm(9, 10, 11),
                    Pre10mm(9, 10, 12),
                    Pre10m(9, 11),
                    Pre10m(9, 12),
                    Post10mm(10, 11),
                    Post10m(11),
                ]
            );
        }
    }
}
