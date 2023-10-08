//! Discover and use PostgreSQL installations.
//!
//! You may have many versions of PostgreSQL installed on a system. For example,
//! on an Ubuntu system, they may be in `/usr/lib/postgresql/*`. On macOS using
//! Homebrew, you may find them in `/usr/local/Cellar/postgresql@*`. [`Runtime`]
//! represents one such runtime; the [`Strategy`] trait represents how to find
//! and select a runtime.

mod cache;
mod error;
pub mod strategies;

use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util;
use crate::version;
pub use error::RuntimeError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runtime {
    /// Path to the directory containing the `pg_ctl` executable and other
    /// PostgreSQL binaries.
    pub bindir: PathBuf,

    /// Version of this runtime.
    pub version: version::Version,
}

impl Runtime {
    pub fn new<P: AsRef<Path>>(bindir: P) -> Result<Self, RuntimeError> {
        let version = cache::version(bindir.as_ref().join("pg_ctl"))?;
        Ok(Self { bindir: bindir.as_ref().to_owned(), version })
    }

    /// Return a [`Command`] prepped to run the given `program` in this
    /// PostgreSQL runtime.
    ///
    /// ```rust
    /// # use postgresfixture::runtime::{self, RuntimeError, Strategy};
    /// # let runtime = runtime::strategies::default().fallback().unwrap();
    /// let version = runtime.execute("pg_ctl").arg("--version").output()?;
    /// # Ok::<(), RuntimeError>(())
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
    /// # use postgresfixture::runtime::{self, RuntimeError, Strategy};
    /// # let runtime = runtime::strategies::default().fallback().unwrap();
    /// let version = runtime.command("bash").arg("-c").arg("echo hello").output();
    /// # Ok::<(), RuntimeError>(())
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

type Runtimes<'a> = Box<dyn Iterator<Item = Runtime> + 'a>;

/// A strategy for finding PostgreSQL runtimes.
///
/// There are a few questions we want to answer:
///
/// 1. What runtimes are available?
/// 2. Which of those runtimes is best suited to running a given cluster?
/// 3. When there are no version constraints, what runtime should we use?
///
/// This trait models those questions, and provides default implementations for
/// #2 and #3.
///
/// A good place to start is [`strategies::default()`] – it might do what you
/// need.
pub trait Strategy: std::panic::RefUnwindSafe + 'static {
    /// Find all runtimes that this strategy knows about.
    fn runtimes(&self) -> Runtimes;

    /// Determine the most appropriate runtime known to this strategy for the
    /// given version constraint.
    ///
    /// The default implementation narrows the list of runtimes to those that
    /// match the given version constraint, then chooses the one with the
    /// highest version number. It might return [`None`].
    fn select(&self, version: &version::PartialVersion) -> Option<Runtime> {
        self.runtimes()
            .filter(|runtime| version.compatible(runtime.version))
            .max_by(|ra, rb| ra.version.cmp(&rb.version))
    }

    /// The runtime to use when there are no version constraints, e.g. when
    /// creating a new cluster.
    ///
    /// The default implementation selects the runtime with the highest version
    /// number.
    fn fallback(&self) -> Option<Runtime> {
        self.runtimes().max_by(|ra, rb| ra.version.cmp(&rb.version))
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
