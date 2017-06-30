#![allow(dead_code)]

extern crate nix;
extern crate semver;

use std::env;
use std::fs;
use std::io;
use std::process::{Command,Output};
use std::path::{Path,PathBuf};
use semver::{Version,SemVerError};

mod lock;
mod util;

use lock::LockDo;


#[derive(Debug)]
enum VersionError {
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

fn get_version<P: AsRef<Path>>(pg_ctl: P) -> Result<Version, VersionError> {
    // Execute pg_ctl and extract version.
    let version_output = Command::new(pg_ctl.as_ref()).arg("--version").output()?;
    let version_string = String::from_utf8_lossy(&version_output.stdout);
    match version_string.split_whitespace().last() {
        Some(version) => Ok(version.parse()?),
        None => Err(VersionError::Missing),
    }
}


struct PostgreSQL {
    /// Path to the directory containing the `pg_ctl` executable and other
    /// PostgreSQL binaries.
    ///
    /// Can be omitted (i.e. `None`) to search `PATH` only.
    bindir: Option<PathBuf>,

    /// Version number of PostgreSQL.
    ///
    /// https://www.postgresql.org/support/versioning/ shows that
    /// version numbers are essentially SemVer compatible... I think.
    version: Version,
}

impl PostgreSQL {

    pub fn default() -> Result<Self, VersionError> {
        Ok(Self{
            bindir: None,
            version: get_version("pg_ctl")?,
        })
    }

    pub fn new<P: AsRef<Path>>(bindir: P) -> Result<Self, VersionError> {
        Ok(Self{
            bindir: Some(bindir.as_ref().to_path_buf()),
            version: get_version(bindir.as_ref().join("pg_ctl"))?,
        })
    }

    pub fn ctl(&self) -> Command {
        let mut command;
        match self.bindir {
            Some(ref bindir) => {
                command = Command::new(bindir.join("pg_ctl"));
                // For now, panic if we can't manipulate PATH.
                // TODO: Print warning if this fails.
                command.env(
                    "PATH", util::prepend_to_path(
                        &bindir, env::var_os("PATH")).unwrap());
            },
            None => {
                command = Command::new("pg_ctl");
            }
        }
        command
    }
}


#[derive(Debug)]
enum ClusterError {
    IoError(io::Error),
    Unsupported(Version),
    Other(Output),
}

impl From<io::Error> for ClusterError {
    fn from(error: io::Error) -> ClusterError {
        ClusterError::IoError(error)
    }
}


struct Cluster {
    /// The data directory of the cluster.
    ///
    /// Corresponds to the `PGDATA` environment variable.
    datadir: PathBuf,
    /// Lock file.
    lockfile: PathBuf,
    /// The installation of PostgreSQL to use with this cluster.
    postgres: PostgreSQL,
}

impl Cluster {

    pub fn new<P: AsRef<Path>>(datadir: P, postgres: PostgreSQL) -> Self {
        let datadir = datadir.as_ref();
        Self{
            datadir: datadir.to_path_buf(),
            lockfile: datadir.parent().unwrap_or(datadir).join(".cluster.lock").to_path_buf(),
            postgres: postgres,
        }
    }

    fn ctl(&self) -> Command {
        let mut command = self.postgres.ctl();
        command.env("PGDATA", &self.datadir);
        command.env("PGHOST", &self.datadir);
        command
    }

    pub fn exists(&self) -> bool {
        self.datadir.is_dir() &&
            self.datadir.join("PG_VERSION").is_file()
    }

    /// Check if this cluster is running.
    ///
    /// Tries to distinguish carefully between "definitely running", "definitely not running", and
    /// "don't know". The latter results in `ClusterError`.
    pub fn is_running(&self) -> Result<bool, ClusterError> {
        // TODO: Test this stuff. It's untested because I want the means to create and destroy
        // clusters before writing the tests for this.
        let output = self.ctl().arg("status").output()?;
        let code = match output.status.code() {
            // Killed by signal; return early.
            None => return Err(ClusterError::Other(output)),
            // Success; return early (the server is running).
            Some(code) if code == 0 => return Ok(true),
            // More work required to decode what this means.
            Some(code) => code,
        };
        let version = &self.postgres.version;
        // PostgreSQL has evolved to return different error codes in
        // later versions, so here we check for specific codes to avoid
        // masking errors from insufficient permissions or missing
        // executables, for example.
        let running = match version.major {
            // PostgreSQL 10.x
            10 => {
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
            },
            // PostgreSQL 9.x
            9 => {
                // PostgreSQL 9.4+
                // https://www.postgresql.org/docs/9.4/static/app-pg-ctl.html
                // https://www.postgresql.org/docs/9.5/static/app-pg-ctl.html
                // https://www.postgresql.org/docs/9.6/static/app-pg-ctl.html
                if version.minor >= 4 {
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
                else if version.minor >= 2 {
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
            },
            // All other versions.
            _ => None,
        };

        match running {
            Some(running) => Ok(running),
            None => Err(ClusterError::Unsupported(version.clone())),
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

    /// Return an open `File` for this cluster's lock file.
    fn lock(&self) -> io::Result<fs::File> {
        fs::OpenOptions::new().append(true).create(true).open(&self.lockfile)
    }

    /// Create the cluster if it does not already exist.
    pub fn create(&self) -> Result<bool, ClusterError> {
        self.lock()?.do_exclusive(|| {
            if !self.datadir.is_dir() {
                fs::create_dir_all(&self.datadir)?;
            }
            match self.datadir.join("PG_VERSION").is_file() {
                // Nothing more to do; the cluster is already in place.
                true => Ok(false),
                // Create the cluster and report back that we had to do so.
                false => {
                    self.ctl().args(&["init", "-s", "-o", "-E utf8 -A trust"]).output()?;
                    Ok(true)
                },
            }
        })?
    }

}


#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::Cluster;
    use super::PostgreSQL;

    use std::env;
    use std::fs::File;
    use std::path::{Path,PathBuf};

    fn find_bindir() -> PathBuf {
        env::split_paths(&env::var_os("PATH").expect("PATH not set"))
            .find(|path| path.join("pg_ctl").exists()).expect("pg_ctl not on PATH")
    }

    #[test]
    fn postgres_new_discovers_version() {
        let pg = PostgreSQL::new(find_bindir()).unwrap();
        assert!(pg.version.major >= 9);
    }

    #[test]
    fn postgres_default_discovers_version() {
        let pg = PostgreSQL::default().unwrap();
        assert!(pg.version.major >= 9);
    }

    #[test]
    fn cluster_new() {
        let pg = PostgreSQL{bindir: None, version: "9.5.2".parse().unwrap()};
        let cluster = Cluster::new("some/path", pg);
        assert_eq!(Path::new("some/path"), cluster.datadir);
        assert_eq!(false, cluster.is_running().unwrap());
    }

    #[test]
    fn cluster_does_not_exist() {
        let pg = PostgreSQL{bindir: None, version: "1.2.3".parse().unwrap()};
        let cluster = Cluster::new("some/path", pg);
        assert!(!cluster.exists());
    }

    #[test]
    fn cluster_does_exist() {
        let data_dir = tempdir::TempDir::new("data").unwrap();
        let version_file = data_dir.path().join("PG_VERSION");
        File::create(&version_file).unwrap();
        let pg = PostgreSQL{bindir: None, version: "1.2.3".parse().unwrap()};
        let cluster = Cluster::new(&data_dir, pg);
        assert!(cluster.exists());
    }

    #[test]
    fn cluster_has_pid_file() {
        let data_dir = PathBuf::from("/some/where");
        let pg = PostgreSQL{bindir: None, version: "1.2.3".parse().unwrap()};
        let cluster = Cluster::new(&data_dir, pg);
        assert_eq!(PathBuf::from("/some/where/postmaster.pid"), cluster.pidfile());
    }

    #[test]
    fn cluster_has_log_file() {
        let data_dir = PathBuf::from("/some/where");
        let pg = PostgreSQL{bindir: None, version: "1.2.3".parse().unwrap()};
        let cluster = Cluster::new(&data_dir, pg);
        assert_eq!(PathBuf::from("/some/where/backend.log"), cluster.logfile());
    }

    #[test]
    fn cluster_create_creates_cluster() {
        let data_dir = tempdir::TempDir::new("data").unwrap();
        let pg = PostgreSQL::default().unwrap();
        let cluster = Cluster::new(&data_dir, pg);
        assert!(!cluster.exists());
        assert!(cluster.create().unwrap());
        assert!(cluster.exists());
    }

    #[test]
    fn cluster_create_does_nothing_when_it_already_exists() {
        let data_dir = tempdir::TempDir::new("data").unwrap();
        let pg = PostgreSQL::default().unwrap();
        let cluster = Cluster::new(&data_dir, pg);
        assert!(!cluster.exists());
        assert!(cluster.create().unwrap());
        assert!(cluster.exists());
        assert!(!cluster.create().unwrap());
    }
}
