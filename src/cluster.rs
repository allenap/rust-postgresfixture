//! Create, start, introspect, stop, and destroy PostgreSQL clusters.

use std::ffi::{OsStr, OsString};
use std::os::unix::prelude::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};
use std::{env, error, fmt, fs, io};

use nix::errno::Errno;
use shell_quote::sh::escape_into;

use crate::runtime;
use crate::version;

#[derive(Debug)]
pub enum ClusterError {
    PathEncodingError, // Path is not UTF-8.
    IoError(io::Error),
    UnixError(nix::Error),
    UnsupportedVersion(version::Version),
    UnknownVersion(version::VersionError),
    DatabaseError(postgres::error::Error),
    InUse, // Cluster is already in use; cannot lock exclusively.
    Other(Output),
}

impl fmt::Display for ClusterError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use ClusterError::*;
        match *self {
            PathEncodingError => write!(fmt, "path is not UTF-8"),
            IoError(ref e) => write!(fmt, "input/output error: {}", e),
            UnixError(ref e) => write!(fmt, "UNIX error: {}", e),
            UnsupportedVersion(ref e) => write!(fmt, "PostgreSQL version not supported: {}", e),
            UnknownVersion(ref e) => write!(fmt, "PostgreSQL version not known: {}", e),
            DatabaseError(ref e) => write!(fmt, "database error: {}", e),
            InUse => write!(fmt, "cluster in use; cannot lock exclusively"),
            Other(ref e) => write!(fmt, "external command failed: {:?}", e),
        }
    }
}

impl error::Error for ClusterError {
    fn cause(&self) -> Option<&dyn error::Error> {
        match *self {
            ClusterError::PathEncodingError => None,
            ClusterError::IoError(ref error) => Some(error),
            ClusterError::UnixError(ref error) => Some(error),
            ClusterError::UnsupportedVersion(_) => None,
            ClusterError::UnknownVersion(ref error) => Some(error),
            ClusterError::DatabaseError(ref error) => Some(error),
            ClusterError::InUse => None,
            ClusterError::Other(_) => None,
        }
    }
}

impl From<io::Error> for ClusterError {
    fn from(error: io::Error) -> ClusterError {
        ClusterError::IoError(error)
    }
}

impl From<nix::Error> for ClusterError {
    fn from(error: nix::Error) -> ClusterError {
        ClusterError::UnixError(error)
    }
}

impl From<version::VersionError> for ClusterError {
    fn from(error: version::VersionError) -> ClusterError {
        ClusterError::UnknownVersion(error)
    }
}

impl From<postgres::error::Error> for ClusterError {
    fn from(error: postgres::error::Error) -> ClusterError {
        ClusterError::DatabaseError(error)
    }
}

impl From<runtime::RuntimeError> for ClusterError {
    fn from(error: runtime::RuntimeError) -> ClusterError {
        match error {
            runtime::RuntimeError::IoError(error) => ClusterError::IoError(error),
            runtime::RuntimeError::UnknownVersion(error) => ClusterError::UnknownVersion(error),
        }
    }
}

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
    pub fn new<P: AsRef<Path>>(datadir: P, runtime: runtime::Runtime) -> Self {
        Self {
            datadir: datadir.as_ref().to_path_buf(),
            runtime,
        }
    }

    fn ctl(&self) -> Command {
        let mut command = self.runtime.execute("pg_ctl");
        command.env("PGDATA", &self.datadir);
        command.env("PGHOST", &self.datadir);
        command
    }

    /// A fairly simplistic check: does the data directory exist and does it
    /// contain a file named `PG_VERSION`?
    pub fn exists(&self) -> bool {
        self.datadir.is_dir() && self.datadir.join("PG_VERSION").is_file()
    }

    /// Returns the PostgreSQL version in use by a cluster.
    ///
    /// This returns the version from the file named `PG_VERSION` in the data
    /// directory if it exists, otherwise this returns `None`. For PostgreSQL
    /// versions before 10 this is typically (maybe always) the major and point
    /// version, e.g. 9.4 rather than 9.4.26. For version 10 and above it
    /// appears to be just the major number, e.g. 14 rather than 14.2.
    pub fn version(&self) -> Result<Option<version::PartialVersion>, ClusterError> {
        let version_file = self.datadir.join("PG_VERSION");
        match std::fs::read_to_string(version_file) {
            Ok(version) => Ok(Some(version.parse()?)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err)?,
        }
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
            Some(code) if code == 0 => return Ok(true),
            // More work required to decode what this means.
            Some(code) => code,
        };
        let version = self.runtime.version()?;
        // PostgreSQL has evolved to return different error codes in
        // later versions, so here we check for specific codes to avoid
        // masking errors from insufficient permissions or missing
        // executables, for example.
        let running = match version {
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
                    4 if !self.exists() => Some(false),
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
                        4 if !self.exists() => Some(false),
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
            None => Err(ClusterError::UnsupportedVersion(version)),
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
        self.datadir.join("backend.log")
    }

    /// Create the cluster if it does not already exist.
    pub fn create(&self) -> Result<bool, ClusterError> {
        match self._create() {
            Err(ClusterError::UnixError(Errno::EAGAIN)) if self.exists() => Ok(false),
            Err(ClusterError::UnixError(Errno::EAGAIN)) => Err(ClusterError::InUse),
            other => other,
        }
    }

    fn _create(&self) -> Result<bool, ClusterError> {
        match self.exists() {
            // Nothing more to do; the cluster is already in place.
            true => Ok(false),
            // Create the cluster and report back that we did so.
            false => {
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

#[cfg(test)]
mod tests {
    use super::Cluster;
    use crate::runtime::Runtime;
    use crate::version::{PartialVersion, Version};

    use std::collections::HashSet;
    use std::fs::File;
    use std::path::{Path, PathBuf};
    use std::str::FromStr;

    #[test]
    fn cluster_new() {
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let cluster = Cluster::new("some/path", runtime);
            assert_eq!(Path::new("some/path"), cluster.datadir);
            assert!(!cluster.running().unwrap());
        }
    }

    #[test]
    fn cluster_does_not_exist() {
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let cluster = Cluster::new("some/path", runtime);
            assert!(!cluster.exists());
        }
    }

    #[test]
    fn cluster_does_exist() {
        let data_dir = tempdir::TempDir::new("data").unwrap();
        let version_file = data_dir.path().join("PG_VERSION");
        File::create(&version_file).unwrap();
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let cluster = Cluster::new(&data_dir, runtime);
            assert!(cluster.exists());
        }
    }

    #[test]
    fn cluster_has_no_version_when_it_does_not_exist() {
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let cluster = Cluster::new("some/path", runtime);
            assert!(matches!(cluster.version(), Ok(None)));
        }
    }

    #[test]
    fn cluster_has_version_when_it_does_exist() {
        let data_dir = tempdir::TempDir::new("data").unwrap();
        let version_file = data_dir.path().join("PG_VERSION");
        File::create(&version_file).unwrap();
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let version: PartialVersion = runtime.version().unwrap().into();
            let version = version.widened();
            std::fs::write(&version_file, format!("{version}\n")).unwrap();
            let cluster = Cluster::new(&data_dir, runtime);
            assert!(matches!(cluster.version(), Ok(Some(_))));
        }
    }

    #[test]
    fn cluster_has_pid_file() {
        let data_dir = PathBuf::from("/some/where");
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let cluster = Cluster::new(&data_dir, runtime);
            assert_eq!(
                PathBuf::from("/some/where/postmaster.pid"),
                cluster.pidfile()
            );
        }
    }

    #[test]
    fn cluster_has_log_file() {
        let data_dir = PathBuf::from("/some/where");
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let cluster = Cluster::new(&data_dir, runtime);
            assert_eq!(PathBuf::from("/some/where/backend.log"), cluster.logfile());
        }
    }

    #[test]
    fn cluster_create_creates_cluster() {
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let data_dir = tempdir::TempDir::new("data").unwrap();
            let cluster = Cluster::new(&data_dir, runtime);
            assert!(!cluster.exists());
            assert!(cluster.create().unwrap());
            assert!(cluster.exists());
        }
    }

    #[test]
    fn cluster_create_creates_cluster_with_neutral_locale_and_timezone() {
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let data_dir = tempdir::TempDir::new("data").unwrap();
            let cluster = Cluster::new(&data_dir, runtime.clone());
            cluster.start().unwrap();
            let mut conn = cluster.connect("postgres").unwrap();
            let result = conn.query("SHOW ALL", &[]).unwrap();
            let params: std::collections::HashMap<String, String> = result
                .into_iter()
                .map(|row| (row.get::<usize, String>(0), row.get::<usize, String>(1)))
                .collect();
            // PostgreSQL 9.4.22's release notes reveal:
            //
            //   Etc/UCT is now a backward-compatibility link to Etc/UTC,
            //   instead of being a separate zone that generates the
            //   abbreviation UCT, which nowadays is typically a typo.
            //   PostgreSQL will still accept UCT as an input zone abbreviation,
            //   but it won't output it.
            //     -- https://www.postgresql.org/docs/9.4/release-9-4-22.html
            //
            if runtime.version().unwrap() < Version::from_str("9.4.22").unwrap() {
                let dealias = |tz: &String| (if tz == "UCT" { "UTC" } else { tz }).to_owned();
                assert_eq!(params.get("TimeZone").map(dealias), Some("UTC".into()));
                assert_eq!(params.get("log_timezone").map(dealias), Some("UTC".into()));
            } else {
                assert_eq!(params.get("TimeZone"), Some(&"UTC".into()));
                assert_eq!(params.get("log_timezone"), Some(&"UTC".into()));
            }
            // PostgreSQL 16's release notes reveal:
            //
            //   Remove read-only server variables lc_collate and lc_ctype …
            //   Collations and locales can vary between databases so having
            //   them as read-only server variables was unhelpful.
            //     -- https://www.postgresql.org/docs/16/release-16.html
            //
            if runtime.version().unwrap() >= Version::from_str("16.0").unwrap() {
                assert_eq!(params.get("lc_collate"), None);
                assert_eq!(params.get("lc_ctype"), None);
                // 🚨 Also in PostgreSQL 16, lc_messages is now the empty string
                // when specified as "C" via any mechanism:
                //
                // - Explicitly given to `initdb`, e.g. `initdb --locale=C`,
                //   `initdb --lc-messages=C`.
                //
                // - Inherited from the environment (LC_ALL, LC_MESSAGES) at any
                //   point (`initdb`, `pg_ctl start`, or from the client).
                //
                // When a different locale is used with `initdb --locale` or
                // `initdb --lc-messages`, e.g. POSIX, es_ES, the locale IS
                // used; lc_messages reflects the choice.
                //
                // It's not yet clear if this is a bug or intentional.
                // https://www.postgresql.org/message-id/18136-4914128da6cfc502%40postgresql.org
                assert_eq!(params.get("lc_messages"), Some(&"".into()));
            } else {
                assert_eq!(params.get("lc_collate"), Some(&"C".into()));
                assert_eq!(params.get("lc_ctype"), Some(&"C".into()));
                assert_eq!(params.get("lc_messages"), Some(&"C".into()));
            }
            assert_eq!(params.get("lc_monetary"), Some(&"C".into()));
            assert_eq!(params.get("lc_numeric"), Some(&"C".into()));
            assert_eq!(params.get("lc_time"), Some(&"C".into()));
            cluster.stop().unwrap();
        }
    }

    #[test]
    fn cluster_create_does_nothing_when_it_already_exists() {
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let data_dir = tempdir::TempDir::new("data").unwrap();
            let cluster = Cluster::new(&data_dir, runtime);
            assert!(!cluster.exists());
            assert!(cluster.create().unwrap());
            assert!(cluster.exists());
            assert!(!cluster.create().unwrap());
        }
    }

    #[test]
    fn cluster_start_stop_starts_and_stops_cluster() {
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let data_dir = tempdir::TempDir::new("data").unwrap();
            let cluster = Cluster::new(&data_dir, runtime);
            cluster.create().unwrap();
            assert!(!cluster.running().unwrap());
            cluster.start().unwrap();
            assert!(cluster.running().unwrap());
            cluster.stop().unwrap();
            assert!(!cluster.running().unwrap());
        }
    }

    #[test]
    fn cluster_destroy_stops_and_removes_cluster() {
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let data_dir = tempdir::TempDir::new("data").unwrap();
            let cluster = Cluster::new(&data_dir, runtime);
            cluster.create().unwrap();
            cluster.start().unwrap();
            assert!(cluster.exists());
            cluster.destroy().unwrap();
            assert!(!cluster.exists());
        }
    }

    #[test]
    fn cluster_connect_connects() {
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let data_dir = tempdir::TempDir::new("data").unwrap();
            let cluster = Cluster::new(&data_dir, runtime);
            cluster.start().unwrap();
            cluster.connect("template1").unwrap();
            cluster.destroy().unwrap();
        }
    }

    #[test]
    fn cluster_databases_returns_vec_of_database_names() {
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let data_dir = tempdir::TempDir::new("data").unwrap();
            let cluster = Cluster::new(&data_dir, runtime);
            cluster.start().unwrap();

            let expected: HashSet<String> = ["postgres", "template0", "template1"]
                .iter()
                .cloned()
                .map(|s| s.to_string())
                .collect();
            let observed: HashSet<String> = cluster.databases().unwrap().iter().cloned().collect();
            assert_eq!(expected, observed);

            cluster.destroy().unwrap();
        }
    }

    #[test]
    fn cluster_databases_with_non_plain_names_can_be_created_and_dropped() {
        // PostgreSQL identifiers containing hyphens, for example, or where we
        // want to preserve capitalisation, are possible.
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let data_dir = tempdir::TempDir::new("data").unwrap();
            let cluster = Cluster::new(&data_dir, runtime);
            cluster.start().unwrap();
            cluster.createdb("foo-bar").unwrap();
            cluster.createdb("Foo-BAR").unwrap();

            let expected: HashSet<String> =
                ["foo-bar", "Foo-BAR", "postgres", "template0", "template1"]
                    .iter()
                    .cloned()
                    .map(|s| s.to_string())
                    .collect();
            let observed: HashSet<String> = cluster.databases().unwrap().iter().cloned().collect();
            assert_eq!(expected, observed);

            cluster.dropdb("foo-bar").unwrap();
            cluster.dropdb("Foo-BAR").unwrap();
            cluster.destroy().unwrap();
        }
    }
}
