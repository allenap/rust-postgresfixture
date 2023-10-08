//! Create, start, introspect, stop, and destroy PostgreSQL clusters.

mod error;

#[cfg(test)]
mod tests;

use std::ffi::{OsStr, OsString};
use std::os::unix::prelude::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::{env, fs, io};

use nix::errno::Errno;
use shell_quote::sh::escape_into;

use crate::runtime;
use crate::version;
pub use error::ClusterError;

/// Representation of a PostgreSQL cluster.
///
/// The cluster may not yet exist on disk. It may exist but be stopped, or it
/// may be running. The methods here can be used to create, start, introspect,
/// stop, and destroy the cluster. There's no protection against concurrent
/// changes to the cluster made by other processes, but the functions in the
/// [`coordinate`][`crate::coordinate`] module may help.
pub struct Cluster {
    /// The data directory of the cluster.
    ///
    /// Corresponds to the `PGDATA` environment variable.
    datadir: PathBuf,
    /// The installation of PostgreSQL to use with this cluster.
    runtime: runtime::Runtime,
}

impl Cluster {
    /// Represent a cluster at the given path.
    ///
    /// This will use the given strategy to determine an appropriate runtime to
    /// use with the cluster in the given data directory, if it exists. If an
    /// appropriate runtime cannot be found, [`ClusterError::RuntimeNotFound`]
    /// will be returned.
    pub fn new<P: AsRef<Path>, S: runtime::strategy::RuntimeStrategy>(
        datadir: P,
        strategy: &S,
    ) -> Result<Self, ClusterError> {
        let datadir = datadir.as_ref();
        let version = version(datadir)?;
        let runtime = match version {
            None => strategy
                .fallback()
                .ok_or_else(|| ClusterError::RuntimeDefaultNotFound),
            Some(version) => strategy
                .select(&version)
                .ok_or_else(|| ClusterError::RuntimeNotFound(version)),
        }?;
        Ok(Self {
            datadir: datadir.to_owned(),
            runtime,
        })
    }

    fn ctl(&self) -> Command {
        let mut command = self.runtime.execute("pg_ctl");
        command.env("PGDATA", &self.datadir);
        command.env("PGHOST", &self.datadir);
        command
    }

    /// Check if this cluster is running.
    ///
    /// Tries to distinguish carefully between "definitely running", "definitely
    /// not running", and "don't know". The latter results in `ClusterError`.
    pub fn running(&self) -> Result<bool, ClusterError> {
        let output = self.ctl().arg("status").output()?;
        let code = match output.status.code() {
            // Killed by signal; return early.
            None => return Err(ClusterError::Other(output)),
            // Success; return early (the server is running).
            Some(0) => return Ok(true),
            // More work required to decode what this means.
            Some(code) => code,
        };
        // PostgreSQL has evolved to return different error codes in
        // later versions, so here we check for specific codes to avoid
        // masking errors from insufficient permissions or missing
        // executables, for example.
        let running = match self.runtime.version {
            // PostgreSQL 10.x and later.
            version::Version::Post10(_major, _minor) => {
                // PostgreSQL 10
                // https://www.postgresql.org/docs/10/static/app-pg-ctl.html
                match code {
                    // 3 means that the data directory is present and
                    // accessible but that the server is not running.
                    3 => Some(false),
                    // 4 means that the data directory is not present or is
                    // not accessible. If it's missing, then the server is
                    // not running. If it is present but not accessible
                    // then crash because we can't know if the server is
                    // running or not.
                    4 if !exists(self) => Some(false),
                    // For anything else we don't know.
                    _ => None,
                }
            }
            // PostgreSQL 9.x only.
            version::Version::Pre10(9, point, _minor) => {
                // PostgreSQL 9.4+
                // https://www.postgresql.org/docs/9.4/static/app-pg-ctl.html
                // https://www.postgresql.org/docs/9.5/static/app-pg-ctl.html
                // https://www.postgresql.org/docs/9.6/static/app-pg-ctl.html
                if point >= 4 {
                    match code {
                        // 3 means that the data directory is present and
                        // accessible but that the server is not running.
                        3 => Some(false),
                        // 4 means that the data directory is not present or is
                        // not accessible. If it's missing, then the server is
                        // not running. If it is present but not accessible
                        // then crash because we can't know if the server is
                        // running or not.
                        4 if !exists(self) => Some(false),
                        // For anything else we don't know.
                        _ => None,
                    }
                }
                // PostgreSQL 9.2+
                // https://www.postgresql.org/docs/9.2/static/app-pg-ctl.html
                // https://www.postgresql.org/docs/9.3/static/app-pg-ctl.html
                else if point >= 2 {
                    match code {
                        // 3 means that the data directory is present and
                        // accessible but that the server is not running OR
                        // that the data directory is not present.
                        3 => Some(false),
                        // For anything else we don't know.
                        _ => None,
                    }
                }
                // PostgreSQL 9.0+
                // https://www.postgresql.org/docs/9.0/static/app-pg-ctl.html
                // https://www.postgresql.org/docs/9.1/static/app-pg-ctl.html
                else {
                    match code {
                        // 1 means that the server is not running OR the data
                        // directory is not present OR that the data directory
                        // is not accessible.
                        1 => Some(false),
                        // For anything else we don't know.
                        _ => None,
                    }
                }
            }
            // All other versions.
            version::Version::Pre10(_major, _point, _minor) => None,
        };

        match running {
            Some(running) => Ok(running),
            // TODO: Perhaps include the exit code from `pg_ctl status` in the
            // error message, and whatever it printed out.
            None => Err(ClusterError::UnsupportedVersion(self.runtime.version)),
        }
    }

    /// Return the path to the PID file used in this cluster.
    ///
    /// The PID file does not necessarily exist.
    pub fn pidfile(&self) -> PathBuf {
        self.datadir.join("postmaster.pid")
    }

    /// Return the path to the log file used in this cluster.
    ///
    /// The log file does not necessarily exist.
    pub fn logfile(&self) -> PathBuf {
        self.datadir.join("postmaster.log")
    }

    /// Create the cluster if it does not already exist.
    pub fn create(&self) -> Result<bool, ClusterError> {
        match self._create() {
            Err(ClusterError::UnixError(Errno::EAGAIN)) if exists(self) => Ok(false),
            Err(ClusterError::UnixError(Errno::EAGAIN)) => Err(ClusterError::InUse),
            other => other,
        }
    }

    fn _create(&self) -> Result<bool, ClusterError> {
        if exists(self) {
            // Nothing more to do; the cluster is already in place.
            Ok(false)
        } else {
            // Create the cluster and report back that we did so.
            fs::create_dir_all(&self.datadir)?;
            #[allow(clippy::suspicious_command_arg_space)]
            self.ctl()
                .arg("init")
                .arg("-s")
                .arg("-o")
                // Passing multiple flags in a single `arg(...)` is
                // intentional. These constitute the single value for the
                // `-o` flag above.
                .arg("-E utf8 --locale C -A trust")
                .env("TZ", "UTC")
                .output()?;
            Ok(true)
        }
    }

    // Start the cluster if it's not already running.
    pub fn start(&self) -> Result<bool, ClusterError> {
        match self._start() {
            Err(ClusterError::UnixError(Errno::EAGAIN)) if self.running()? => Ok(false),
            Err(ClusterError::UnixError(Errno::EAGAIN)) => Err(ClusterError::InUse),
            other => other,
        }
    }

    fn _start(&self) -> Result<bool, ClusterError> {
        // Ensure that the cluster has been created.
        self._create()?;
        // Check if we're running already.
        if self.running()? {
            // We didn't start this cluster; say so.
            return Ok(false);
        }
        // Next, invoke `pg_ctl` to start the cluster.
        // pg_ctl options:
        //  -l <file> -- log file.
        //  -s -- no informational messages.
        //  -w -- wait until startup is complete.
        // postgres options:
        //  -h <arg> -- host name; empty arg means Unix socket only.
        //  -k -- socket directory.
        self.ctl()
            .arg("start")
            .arg("-l")
            .arg(self.logfile())
            .arg("-s")
            .arg("-w")
            .arg("-o")
            .arg({
                let mut arg = b"-h '' -k "[..].into();
                escape_into(&self.datadir, &mut arg);
                OsString::from_vec(arg)
            })
            .output()?;
        // We did actually start the cluster; say so.
        Ok(true)
    }

    // Connect to this cluster.
    pub fn connect(&self, database: &str) -> Result<postgres::Client, ClusterError> {
        let user = &env::var("USER").unwrap_or_else(|_| "USER-not-set".to_string());
        let host = self.datadir.to_string_lossy(); // postgres crate API limitation.
        let client = postgres::Client::configure()
            .user(user)
            .dbname(database)
            .host(&host)
            .connect(postgres::NoTls)?;
        Ok(client)
    }

    pub fn shell(&self, database: &str) -> Result<ExitStatus, ClusterError> {
        let mut command = self.runtime.execute("psql");
        command.arg("--quiet");
        command.env("PGDATA", &self.datadir);
        command.env("PGHOST", &self.datadir);
        command.env("PGDATABASE", database);
        Ok(command.spawn()?.wait()?)
    }

    pub fn exec<T: AsRef<OsStr>>(
        &self,
        database: &str,
        command: T,
        args: &[T],
    ) -> Result<ExitStatus, ClusterError> {
        let mut command = self.runtime.command(command);
        command.args(args);
        command.env("PGDATA", &self.datadir);
        command.env("PGHOST", &self.datadir);
        command.env("PGDATABASE", database);
        Ok(command.spawn()?.wait()?)
    }

    // The names of databases in this cluster.
    pub fn databases(&self) -> Result<Vec<String>, ClusterError> {
        let mut conn = self.connect("template1")?;
        let rows = conn.query(
            "SELECT datname FROM pg_catalog.pg_database ORDER BY datname",
            &[],
        )?;
        let datnames: Vec<String> = rows.iter().map(|row| row.get(0)).collect();
        Ok(datnames)
    }

    /// Create the named database.
    pub fn createdb(&self, database: &str) -> Result<bool, ClusterError> {
        let statement = format!(
            "CREATE DATABASE {}",
            postgres_protocol::escape::escape_identifier(database)
        );
        self.connect("template1")?
            .execute(statement.as_str(), &[])?;
        Ok(true)
    }

    /// Drop the named database.
    pub fn dropdb(&self, database: &str) -> Result<bool, ClusterError> {
        let statement = format!(
            "DROP DATABASE {}",
            postgres_protocol::escape::escape_identifier(database)
        );
        self.connect("template1")?
            .execute(statement.as_str(), &[])?;
        Ok(true)
    }

    // Stop the cluster if it's running.
    pub fn stop(&self) -> Result<bool, ClusterError> {
        match self._stop() {
            Err(ClusterError::UnixError(Errno::EAGAIN)) if !self.running()? => Ok(false),
            Err(ClusterError::UnixError(Errno::EAGAIN)) => Err(ClusterError::InUse),
            other => other,
        }
    }

    fn _stop(&self) -> Result<bool, ClusterError> {
        // If the cluster's not already running, don't do anything.
        if !self.running()? {
            return Ok(false);
        }
        // pg_ctl options:
        //  -w -- wait for shutdown to complete.
        //  -m <mode> -- shutdown mode.
        self.ctl()
            .arg("stop")
            .arg("-s")
            .arg("-w")
            .arg("-m")
            .arg("fast")
            .output()?;
        Ok(true)
    }

    // Destroy the cluster if it exists, after stopping it.
    pub fn destroy(&self) -> Result<bool, ClusterError> {
        match self._destroy() {
            Err(ClusterError::UnixError(Errno::EAGAIN)) => Err(ClusterError::InUse),
            other => other,
        }
    }

    fn _destroy(&self) -> Result<bool, ClusterError> {
        if self._stop()? || self.datadir.is_dir() {
            fs::remove_dir_all(&self.datadir)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl AsRef<Path> for Cluster {
    fn as_ref(&self) -> &Path {
        &self.datadir
    }
}

/// A fairly simplistic but quick check: does the directory exist and does it
/// look like a PostgreSQL cluster data directory, i.e. does it contain a file
/// named `PG_VERSION`?
///
/// [`version()`] provides a more reliable measure, plus yields the version of
/// PostgreSQL required to use the cluster.
pub fn exists<P: AsRef<Path>>(datadir: P) -> bool {
    let datadir = datadir.as_ref();
    datadir.is_dir() && datadir.join("PG_VERSION").is_file()
}

/// Yields the version of PostgreSQL required to use a cluster.
///
/// This returns the version from the file named `PG_VERSION` in the data
/// directory if it exists, otherwise this returns `None`. For PostgreSQL
/// versions before 10 this is typically (maybe always) the major and point
/// version, e.g. 9.4 rather than 9.4.26. For version 10 and above it appears to
/// be just the major number, e.g. 14 rather than 14.2.
pub fn version<P: AsRef<Path>>(
    datadir: P,
) -> Result<Option<version::PartialVersion>, ClusterError> {
    let version_file = datadir.as_ref().join("PG_VERSION");
    match std::fs::read_to_string(version_file) {
        Ok(version) => Ok(Some(version.parse()?)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err)?,
    }
}
