//! Discover and use PostgreSQL installations.
//!
//! You may have many versions of PostgreSQL installed on a system. For example,
//! on an Ubuntu system, they may be in `/usr/lib/postgresql/*`. On macOS using
//! Homebrew, you may find them in `/usr/local/Cellar/postgresql@*`. [`Runtime`]
//! represents one such runtime; the [`Strategy`] trait represents how to find
//! and select a runtime.

mod cache;
mod error;
pub mod strategy;

use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util;
use crate::version;
pub use error::Error;
pub use strategy::Strategy;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runtime {
    /// Path to the directory containing the `pg_ctl` executable and other
    /// PostgreSQL binaries.
    pub bindir: PathBuf,

    /// Version of this runtime.
    pub version: version::Version,
}

impl Runtime {
    pub fn new<P: AsRef<Path>>(bindir: P) -> Result<Self, Error> {
        let version = cache::version(bindir.as_ref().join("pg_ctl"))?;
        Ok(Self { bindir: bindir.as_ref().to_owned(), version })
    }

    /// Return a [`Command`] prepped to run the given `program` in this
    /// PostgreSQL runtime.
    ///
    /// ```rust
    /// # use postgresfixture::runtime::{self, Runtime, Strategy};
    /// # let runtime = runtime::strategy::default().fallback().unwrap();
    /// let version = runtime.execute("pg_ctl").arg("--version").output()?;
    /// # Ok::<(), runtime::Error>(())
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if it's not possible to calculate `PATH`; see
    /// [`env::join_paths`].
    pub fn execute<T: AsRef<OsStr>>(&self, program: T) -> Command {
        let mut command = Command::new(self.bindir.join(program.as_ref()));
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
    /// # use postgresfixture::runtime::{self, Strategy};
    /// # let runtime = runtime::strategy::default().fallback().unwrap();
    /// let version = runtime.command("bash").arg("-c").arg("echo hello").output();
    /// # Ok::<(), runtime::Error>(())
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if it's not possible to calculate `PATH`; see
    /// [`env::join_paths`].
    pub fn command<T: AsRef<OsStr>>(&self, program: T) -> Command {
        let mut command = Command::new(program);
        command.env(
            "PATH",
            util::prepend_to_path(&self.bindir, env::var_os("PATH")).unwrap(),
        );
        command
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, Runtime};

    use std::env;
    use std::path::PathBuf;

    type TestResult = Result<(), Error>;

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
