use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

pub use crate::version::{Version, VersionError};

use crate::util;

#[derive(Debug)]
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
    /// Find runtimes on the given path.
    ///
    /// Parses input according to platform conventions for the `PATH`
    /// environment variable. See [`env::split_paths`] for details.
    pub fn find<T: AsRef<OsStr> + ?Sized>(path: &T) -> Vec<Self> {
        env::split_paths(path)
            .filter(|bindir| bindir.join("pg_ctl").exists())
            .map(|bindir| Self {
                bindir: Some(bindir),
            })
            .collect()
    }

    /// Find runtimes on `PATH` (environment variable).
    pub fn find_on_path() -> Vec<Self> {
        match env::var_os("PATH") {
            Some(path) => Self::find(&path),
            None => vec![],
        }
    }

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
        // The version parser can deal with leading garbage, i.e. it can parse
        // "pg_ctl (PostgreSQL) 12.2" and get 12.2 out of it.
        Ok(version_string.parse()?)
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
                    util::prepend_to_path(bindir, env::var_os("PATH")).unwrap(),
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
    use super::Runtime;

    use std::env;
    use std::path::PathBuf;

    fn find_bindir() -> PathBuf {
        env::split_paths(&env::var_os("PATH").expect("PATH not set"))
            .find(|path| path.join("pg_ctl").exists())
            .expect("pg_ctl not on PATH")
    }

    #[test]
    fn runtime_find() {
        let path = env::var_os("PATH").expect("PATH not set");
        let pgs = Runtime::find(&path);
        assert_ne!(0, pgs.len());
    }

    #[test]
    fn runtime_find_on_path() {
        let pgs = Runtime::find_on_path();
        assert_ne!(0, pgs.len());
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
