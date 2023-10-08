//! Discover and use PostgreSQL installations.
//!
//! You may have many versions of PostgreSQL installed on a system. For example,
//! on an Ubuntu system, they may be in `/usr/lib/postgresql/*`. On macOS using
//! Homebrew, you may find them in `/usr/local/Cellar/postgresql@*`. [`Runtime`]
//! can traverse your `PATH` to discover all the versions currently available to
//! you.

mod error;
pub mod strategy;

use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util;
use crate::version::{self, VersionError};
pub use error::RuntimeError;

#[derive(Clone, Debug)]
pub struct Runtime {
    /// Path to the directory containing the `pg_ctl` executable and other
    /// PostgreSQL binaries.
    pub bindir: PathBuf,

    /// Version of this runtime.
    pub version: version::Version,
}

impl Runtime {
    pub fn new<P: AsRef<Path>>(bindir: P) -> Result<Self, RuntimeError> {
        Ok(Self {
            bindir: bindir.as_ref().to_owned(),
            version: version(bindir)?,
        })
    }

    /// Return a [`Command`] prepped to run the given `program` in this
    /// PostgreSQL runtime.
    ///
    /// ```rust
    /// # use postgresfixture::runtime::{self, Runtime, strategy::{RuntimeStrategy}};
    /// # let runtime = runtime::strategy::default().fallback().unwrap();
    /// let version = runtime.execute("pg_ctl").arg("--version").output().unwrap();
    /// ```
    pub fn execute<T: AsRef<OsStr>>(&self, program: T) -> Command {
        let mut command = Command::new(self.bindir.join(program.as_ref()));
        // For now, panic if we can't manipulate PATH.
        // TODO: Print warning if this fails.
        command.env(
            "PATH",
            util::prepend_to_path(&self.bindir, env::var_os("PATH")).unwrap(),
        );
        command
    }

    /// Return a [`Command`] prepped to run the given `program` with this
    /// PostgreSQL runtime at the front of `PATH`. This is very similar to
    /// [`Self::execute`] except it does not qualify the given program name with
    /// [`Self::bindir`].
    ///
    /// ```rust
    /// # use postgresfixture::runtime::{self, strategy::RuntimeStrategy};
    /// let runtime = runtime::strategy::default().fallback().unwrap();
    /// let version = runtime.command("bash").arg("-c").arg("echo hello").output().unwrap();
    /// ```
    pub fn command<T: AsRef<OsStr>>(&self, program: T) -> Command {
        let mut command = Command::new(program);
        // For now, panic if we can't manipulate PATH.
        // TODO: Print warning if this fails.
        command.env(
            "PATH",
            util::prepend_to_path(&self.bindir, env::var_os("PATH")).unwrap(),
        );
        command
    }
}

/// Get the version of PostgreSQL from `pg_ctl`.
///
/// The [PostgreSQL "Versioning Policy"][versioning] shows that version numbers
/// are **not** SemVer compatible. The [`version`][`mod@crate::version`] module
/// in this crate is used to parse the version string from `pg_ctl` and it does
/// understand the nuances of PostgreSQL's versioning scheme.
///
/// [versioning]: https://www.postgresql.org/support/versioning/
pub fn version<P: AsRef<Path>>(bindir: P) -> Result<version::Version, RuntimeError> {
    // Execute pg_ctl and extract version.
    let command = bindir.as_ref().join("pg_ctl");
    let output = Command::new(command).arg("--version").output()?;
    if output.status.success() {
        let version_string = String::from_utf8_lossy(&output.stdout);
        // The version parser can deal with leading garbage, i.e. it can parse
        // "pg_ctl (PostgreSQL) 12.2" and get 12.2 out of it.
        Ok(version_string.parse()?)
    } else {
        Err(RuntimeError::UnknownVersion(VersionError::Missing))
    }
}

#[cfg(test)]
mod tests {
    use super::{Runtime, RuntimeError};

    use std::env;
    use std::path::PathBuf;

    type TestResult = Result<(), RuntimeError>;

    fn find_bindir() -> PathBuf {
        env::split_paths(&env::var_os("PATH").expect("PATH not set"))
            .find(|path| path.join("pg_ctl").exists())
            .expect("pg_ctl not on PATH")
    }

    #[test]
    fn runtime_new() -> TestResult {
        let bindir = find_bindir();
        let pg = Runtime::new(&bindir)?;
        assert_eq!(bindir, pg.bindir);
        Ok(())
    }
}
