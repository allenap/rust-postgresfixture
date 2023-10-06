//! Discover and use PostgreSQL installations.
//!
//! You may have many versions of PostgreSQL installed on a system. For example,
//! on an Ubuntu system, they may be in `/usr/lib/postgresql/*`. On macOS using
//! Homebrew, you may find them in `/usr/local/Cellar/postgresql@*`. [`Runtime`]
//! can traverse your `PATH` to discover all the versions currently available to
//! you.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, error, fmt, io};

use crate::util;
use crate::version;

#[derive(Debug)]
pub enum RuntimeError {
    IoError(io::Error),
    UnknownVersion(version::VersionError),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use RuntimeError::*;
        match *self {
            IoError(ref e) => write!(fmt, "input/output error: {}", e),
            UnknownVersion(ref e) => write!(fmt, "PostgreSQL version not known: {}", e),
        }
    }
}

impl error::Error for RuntimeError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            RuntimeError::IoError(ref error) => Some(error),
            RuntimeError::UnknownVersion(ref error) => Some(error),
        }
    }
}

impl From<io::Error> for RuntimeError {
    fn from(error: io::Error) -> RuntimeError {
        RuntimeError::IoError(error)
    }
}

impl From<version::VersionError> for RuntimeError {
    fn from(error: version::VersionError) -> RuntimeError {
        RuntimeError::UnknownVersion(error)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Runtime {
    /// Path to the directory containing the `pg_ctl` executable and other
    /// PostgreSQL binaries.
    ///
    /// Can be omitted (i.e. [`None`]) to search `PATH` only.
    pub bindir: Option<PathBuf>,
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

    /// Find runtimes using platform-specific knowledge (Linux).
    ///
    /// For example: on Debian and Ubuntu, check `/usr/lib/postgresql`.
    #[cfg(any(doc, target_os = "linux"))]
    pub fn find_on_platform() -> Vec<Self> {
        glob::glob("/usr/lib/postgresql/*/bin/pg_ctl")
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter(|path| path.is_file())
                    .filter_map(|path| path.parent().map(Self::new))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find runtimes using platform-specific knowledge (macOS).
    ///
    /// For example: check Homebrew.
    #[cfg(any(doc, target_os = "macos"))]
    pub fn find_on_platform() -> Vec<Self> {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        Command::new("brew")
            .arg("--prefix")
            .output()
            .ok()
            .map(|output| OsString::from_vec(output.stdout))
            .and_then(|brew_prefix| {
                glob::glob(&format!(
                    "{}/Cellar/postgresql@*/*/bin/pg_ctl",
                    brew_prefix.to_string_lossy().trim_end()
                ))
                .ok()
            })
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter(|path| path.is_file())
                    .filter_map(|path| path.parent().map(Self::new))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn new<P: AsRef<Path>>(bindir: P) -> Self {
        Self {
            bindir: Some(bindir.as_ref().to_path_buf()),
        }
    }

    /// Get the version of PostgreSQL from `pg_ctl`.
    ///
    /// <https://www.postgresql.org/support/versioning/> shows that version
    /// numbers are **not** SemVer compatible. The [`version`][`crate::version`]
    /// module in this crate can parse the version string returned by this
    /// function.
    pub fn version(&self) -> Result<version::Version, RuntimeError> {
        // Execute pg_ctl and extract version.
        let version_output = self.execute("pg_ctl").arg("--version").output()?;
        let version_string = String::from_utf8_lossy(&version_output.stdout);
        // The version parser can deal with leading garbage, i.e. it can parse
        // "pg_ctl (PostgreSQL) 12.2" and get 12.2 out of it.
        Ok(version_string.parse()?)
    }

    /// Return a [`Command`] prepped to run the given `program` in this
    /// PostgreSQL runtime.
    ///
    /// ```rust
    /// # use postgresfixture::runtime::Runtime;
    /// let version = Runtime::default().execute("pg_ctl").arg("--version").output().unwrap();
    /// ```
    pub fn execute<T: AsRef<OsStr>>(&self, program: T) -> Command {
        let mut command;
        match self.bindir {
            Some(ref bindir) => {
                command = Command::new(bindir.join(program.as_ref()));
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

    /// Return a [`Command`] prepped to run the given `program` with this
    /// PostgreSQL runtime at the front of `PATH`. This is very similar to
    /// [`Self::execute`] except it does not qualify the given program name with
    /// [`Self::bindir`].
    ///
    /// ```rust
    /// # use postgresfixture::runtime::Runtime;
    /// let version = Runtime::default().command("bash").arg("-c").arg("echo hello").output().unwrap();
    /// ```
    pub fn command<T: AsRef<OsStr>>(&self, program: T) -> Command {
        let mut command;
        match self.bindir {
            Some(ref bindir) => {
                command = Command::new(program);
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

pub fn determine_best_runtime_for_version<RUNTIMES>(
    version: &version::PartialVersion,
    runtimes: RUNTIMES,
) -> Option<Runtime>
where
    RUNTIMES: IntoIterator<Item = Runtime>,
{
    runtimes
        .into_iter()
        .filter_map(|runtime| runtime.version().map(|rtv| (rtv, runtime)).ok())
        .filter(|(rtv, _)| version.compatible(*rtv))
        .max_by(|(v1, _), (v2, _)| v1.cmp(v2))
        .map(|(_, runtime)| runtime)
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
        let runtimes = Runtime::find(&path);
        assert_ne!(0, runtimes.len());
    }

    #[test]
    fn runtime_find_on_path() {
        let runtimes = Runtime::find_on_path();
        assert_ne!(0, runtimes.len());
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn runtime_find_on_platform() {
        let runtimes = Runtime::find_on_platform();
        assert_ne!(0, runtimes.len());
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
