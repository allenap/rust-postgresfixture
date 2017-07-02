use std::env;
use std::io;
use std::process::Command;
use std::path::{Path,PathBuf};

use runtime;
use postgres;
use semver::{Version,SemVerError};
use util;


#[derive(Debug)]
pub enum VersionError {
    IoError(io::Error),
    Invalid(SemVerError),
    Missing,
}

impl From<io::Error> for VersionError {
    fn from(error: io::Error) -> VersionError {
        VersionError::IoError(error)
    }
}

impl From<SemVerError> for VersionError {
    fn from(error: SemVerError) -> VersionError {
        VersionError::Invalid(error)
    }
}


pub struct PostgreSQL {
    /// Path to the directory containing the `pg_ctl` executable and other
    /// PostgreSQL binaries.
    ///
    /// Can be omitted (i.e. `None`) to search `PATH` only.
    bindir: Option<PathBuf>,
}

impl Default for PostgreSQL {
    fn default() -> Self {
        Self{bindir: None}
    }
}

impl PostgreSQL {

    pub fn new<P: AsRef<Path>>(bindir: P) -> Self {
        Self{bindir: Some(bindir.as_ref().to_path_buf())}
    }

    /// Get the version number of PostgreSQL.
    ///
    /// https://www.postgresql.org/support/versioning/ shows that
    /// version numbers are essentially SemVer compatible... I think.
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
                    "PATH", util::prepend_to_path(
                        &bindir, env::var_os("PATH")).unwrap());
            },
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

    use super::Cluster;
    use super::PostgreSQL;

    use std::collections::HashSet;
    use std::env;
    use std::fs::File;
    use std::path::{Path,PathBuf};

    fn find_bindir() -> PathBuf {
        env::split_paths(&env::var_os("PATH").expect("PATH not set"))
            .find(|path| path.join("pg_ctl").exists()).expect("pg_ctl not on PATH")
    }

    #[test]
    fn postgres_new() {
        let bindir = find_bindir();
        let pg = PostgreSQL::new(&bindir);
        assert_eq!(Some(bindir), pg.bindir);
    }

    #[test]
    fn postgres_default() {
        let pg = PostgreSQL::default();
        assert_eq!(None, pg.bindir);
        let pg: PostgreSQL = Default::default();  // Via trait.
        assert_eq!(None, pg.bindir);
    }

}
