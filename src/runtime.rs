use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::{env, error, fmt, io, num};

use regex::Regex;
use util;

#[derive(Debug)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: Option<u32>,
}

#[derive(Debug)]
pub enum VersionParseError {
    Invalid,
    Missing,
}

impl fmt::Display for VersionParseError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", (self as &error::Error).description())
    }
}

impl error::Error for VersionParseError {
    fn description(&self) -> &str {
        match *self {
            VersionParseError::Invalid => "version was badly formed",
            VersionParseError::Missing => "version information not found",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        None
    }
}

impl From<num::ParseIntError> for VersionParseError {
    fn from(_error: num::ParseIntError) -> VersionParseError {
        VersionParseError::Invalid
    }
}

impl FromStr for Version {
    type Err = VersionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"(?x) \b (\d+) [.] (\d+) (?: [.] (\d+) ) \b").unwrap();
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
    Missing,
}

impl fmt::Display for VersionError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", (self as &error::Error).description())
    }
}

impl error::Error for VersionError {
    fn description(&self) -> &str {
        match *self {
            VersionError::IoError(_) => "input/output error",
            VersionError::Invalid(_) => "version was badly formed",
            VersionError::Missing => "version information not found",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            VersionError::IoError(ref error) => Some(error),
            VersionError::Invalid(ref error) => Some(error),
            VersionError::Missing => None,
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

pub struct Runtime {
    /// Path to the directory containing the `pg_ctl` executable and other
    /// PostgreSQL binaries.
    ///
    /// Can be omitted (i.e. `None`) to search `PATH` only.
    pub bindir: Option<PathBuf>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self { bindir: None }
    }
}

impl Runtime {
    pub fn new<P: AsRef<Path>>(bindir: P) -> Self {
        Self {
            bindir: Some(bindir.as_ref().to_path_buf()),
        }
    }

    /// Get the version number of PostgreSQL.
    ///
    /// https://www.postgresql.org/support/versioning/ shows that version
    /// numbers are NOT SemVer compatible, so we have to parse them ourselves.
    pub fn version(&self) -> Result<Version, VersionError> {
        // Execute pg_ctl and extract version.
        let version_output = self.execute("pg_ctl").arg("--version").output()?;
        let version_string = String::from_utf8_lossy(&version_output.stdout);
        match version_string.split_whitespace().last() {
            Some(version) => Ok(version.parse()?),
            None => Err(VersionError::Missing),
        }
    }

    pub fn execute(&self, program: &str) -> Command {
        let mut command;
        match self.bindir {
            Some(ref bindir) => {
                command = Command::new(bindir.join(program));
                // For now, panic if we can't manipulate PATH.
                // TODO: Print warning if this fails.
                command.env(
                    "PATH",
                    util::prepend_to_path(&bindir, env::var_os("PATH")).unwrap(),
                );
            }
            None => {
                command = Command::new(program);
            }
        }
        command
    }
}

#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::Runtime;

    use std::env;
    use std::path::PathBuf;

    fn find_bindir() -> PathBuf {
        env::split_paths(&env::var_os("PATH").expect("PATH not set"))
            .find(|path| path.join("pg_ctl").exists())
            .expect("pg_ctl not on PATH")
    }

    #[test]
    fn runtime_new() {
        let bindir = find_bindir();
        let pg = Runtime::new(&bindir);
        assert_eq!(Some(bindir), pg.bindir);
    }

    #[test]
    fn runtime_default() {
        let pg = Runtime::default();
        assert_eq!(None, pg.bindir);
        let pg: Runtime = Default::default(); // Via trait.
        assert_eq!(None, pg.bindir);
    }
}
