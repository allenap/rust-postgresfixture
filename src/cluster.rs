use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};
use std::{env, error, fmt, fs, io};

use shell_escape::escape;

use crate::lock::LockDo;
use crate::runtime;

#[derive(Debug)]
pub enum ClusterError {
    PathEncodingError, // Path is not UTF-8.
    IoError(io::Error),
    UnixError(nix::Error),
    UnsupportedVersion(runtime::Version),
    UnknownVersion(runtime::VersionError),
    DatabaseError(postgres::error::Error),
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

impl From<runtime::VersionError> for ClusterError {
    fn from(error: runtime::VersionError) -> ClusterError {
        ClusterError::UnknownVersion(error)
    }
}

impl From<postgres::error::Error> for ClusterError {
    fn from(error: postgres::error::Error) -> ClusterError {
        ClusterError::DatabaseError(error)
    }
}

pub struct Cluster {
    /// The data directory of the cluster.
    ///
    /// Corresponds to the `PGDATA` environment variable.
    datadir: PathBuf,
    /// Lock file.
    lockfile: PathBuf,
    /// The installation of PostgreSQL to use with this cluster.
    runtime: runtime::Runtime,
}

/*

Proposed new locking scheme:
----------------------------

- If `DATA` directory does not exist, create it.
  - This may fail because a concurrent process has created it. Continue.
- Lock file `DATA/.postgresfixture.lock`
  - exclusively when creating, starting, stopping, or destroying the cluster.
  - shared when using the cluster.
  - Is it possible to downgrade from exclusive flock to shared? Yes.
    - From _flock(2)_ on Linux:
      A process may hold only one type of lock (shared or exclusive) on a file.  Subsequent flock()
      calls on an already locked file will convert an existing lock to the new lock mode.
    - However, from _flock(2)_ on MacOSX:
        A shared lock may be upgraded to an exclusive lock, and vice versa, simply by specifying
        the appropriate lock type; this results in the previous lock being released and the new
        lock applied (**possibly after other processes have gained and released the lock**).
- When destroying cluster, move `DATA` to `DATA.XXXXXX` (where `XXXXXX` is random) *after*
  stopping the cluster but *before* deleting all files.
  - What happens when process A is destroying a cluster and process B comes to, say, also destroy
    the cluster? The following would be bad:
    - B blocks, waiting for an exclusive lock on `DATA/.postgresfixture.lock`.
    - A moves `DATA` to `DATA.XXXXXX`.
    - A destroys `DATA.XXXXXX` and releases all locks.
    - Meanwhile, process C creates a new cluster in `DATA` and uses it.
    - B still has a file-descriptor for lock file. That lock has been deleted from the filesystem,
      but B can nevertheless now exclusively lock it.
    - B then goes about destroying `DATA` because it thinks it has an exclusive lock.
    - C gets confused, angry.
    - FIX: Ensure that the lock's `File` refers to the same file as a newly-opened `File` for the
      lock. Use the [same-file](https://crates.io/crates/same-file) crate for this:
      `same_file::Handle` instances are equal when they refer to the same underlying file.

*/

impl Cluster {
    pub fn new<P: AsRef<Path>>(datadir: P, runtime: runtime::Runtime) -> Self {
        let datadir = datadir.as_ref();
        Self {
            datadir: datadir.to_path_buf(),
            lockfile: datadir.parent().unwrap_or(datadir).join(".cluster.lock"),
            runtime,
        }
    }

    fn ctl(&self) -> Command {
        let mut command = self.runtime.execute("pg_ctl");
        command.env("PGDATA", &self.datadir);
        command.env("PGHOST", &self.datadir);
        command
    }

    pub fn exists(&self) -> bool {
        self.datadir.is_dir() && self.datadir.join("PG_VERSION").is_file()
    }

    /// Check if this cluster is running.
    ///
    /// Tries to distinguish carefully between "definitely running", "definitely not running", and
    /// "don't know". The latter results in `ClusterError`.
    pub fn running(&self) -> Result<bool, ClusterError> {
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
        let version = self.runtime.version()?;
        // PostgreSQL has evolved to return different error codes in
        // later versions, so here we check for specific codes to avoid
        // masking errors from insufficient permissions or missing
        // executables, for example.
        let running = match (version.major >= 10, version.major) {
            // PostgreSQL 10.x and later.
            (true, _) => {
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
            (false, 9) => {
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
            }
            // All other versions.
            (_, _) => None,
        };

        match running {
            Some(running) => Ok(running),
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

    /// Return an open `File` for this cluster's lock file.
    fn lock(&self) -> io::Result<fs::File> {
        fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.lockfile)
    }

    /// Create the cluster if it does not already exist.
    pub fn create(&self) -> Result<bool, ClusterError> {
        self.lock()?.do_exclusive(|| self._create())?
    }

    fn _create(&self) -> Result<bool, ClusterError> {
        match self.datadir.join("PG_VERSION").is_file() {
            // Nothing more to do; the cluster is already in place.
            true => Ok(false),
            // Create the cluster and report back that we did so.
            false => {
                fs::create_dir_all(&self.datadir)?;
                self.ctl()
                    .arg("init")
                    .arg("-s")
                    .arg("-o")
                    .arg("-E utf8 -A trust")
                    .output()?;
                Ok(true)
            }
        }
    }

    // Start the cluster if it's not already running.
    pub fn start(&self) -> Result<bool, ClusterError> {
        self.lock()?.do_exclusive(|| self._start())?
    }

    fn _start(&self) -> Result<bool, ClusterError> {
        // If the cluster's already running, don't do anything.
        if self.running()? {
            return Ok(false);
        }
        // Ensure that the cluster has been created.
        self._create()?;
        // This next thing is kind of a Rust wart, kind of a wart in the `shell-escape` crate. UNIX
        // paths are all bytes and the only thing they're not allowed to contain is, AFAIK, the
        // null byte. The encoding is defined by the locale variables, again AFAIK, but there's an
        // implicit assumption in Rust that `OsString` and its kin are essentially UTF-8. The
        // `shell-escape` crate only understands `&str` so we have to convert a platform-specific
        // path string to a Rust string in order to use it. Cargo, another user of `shell-escape`,
        // uses `to_string_lossy` here, but I'm choosing to be strict and reject any platform path
        // that's not also valid UTF-8. The question now arises: why do we need `shell-escape`? One
        // of the arguments we will pass to `pg_ctl` will be used as the argument _list_ when it
        // invokes `postgres`. Sucks, but there it is.
        let datadir = self
            .datadir
            .as_path()
            .as_os_str()
            .to_str()
            .ok_or(ClusterError::PathEncodingError)?;
        // Next, invoke `pg_ctl` to start the cluster.
        // pg_ctl options:
        //  -l <file> -- log file.
        //  -s -- no informational messages.
        //  -w -- wait until startup is complete.
        // postgres options:
        //  -h <arg> -- host name; empty arg means Unix socket only.
        //  -F -- don't bother fsync'ing.
        //  -k -- socket directory.
        self.ctl()
            .arg("start")
            .arg("-l")
            .arg(self.logfile())
            .arg("-s")
            .arg("-w")
            .arg("-o")
            .arg(format!("-h '' -F -k {}", escape(datadir.into())))
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
        command.arg("--quiet").arg("--").arg(database);
        command.env("PGDATA", &self.datadir);
        command.env("PGHOST", &self.datadir);
        Ok(command.spawn()?.wait()?)
    }

    // The names of databases in this cluster.
    pub fn databases(&self) -> Result<Vec<String>, ClusterError> {
        let mut conn = self.connect("template1")?;
        let rows = conn.query("SELECT datname FROM pg_catalog.pg_database", &[])?;
        let datnames: Vec<String> = rows.iter().map(|row| row.get(0)).collect();
        Ok(datnames)
    }

    /// Create the named database.
    pub fn createdb(&self, database: &str) -> Result<bool, ClusterError> {
        let statement = format!("CREATE DATABASE {}", &database);
        self.connect("template1")?
            .execute(statement.as_str(), &[])?;
        Ok(true)
    }

    /// Drop the named database.
    pub fn dropdb(&self, database: &str) -> Result<bool, ClusterError> {
        let statement = format!("DROP DATABASE {}", &database);
        self.connect("template1")?
            .execute(statement.as_str(), &[])?;
        Ok(true)
    }

    // Stop the cluster if it's running.
    pub fn stop(&self) -> Result<bool, ClusterError> {
        self.lock()?.do_exclusive(|| self._stop())?
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
        self.lock()?.do_exclusive(|| self._destroy())?
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

    use std::collections::HashSet;
    use std::fs::File;
    use std::path::{Path, PathBuf};

    #[test]
    fn cluster_new() {
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let cluster = Cluster::new("some/path", runtime);
            assert_eq!(Path::new("some/path"), cluster.datadir);
            assert_eq!(false, cluster.running().unwrap());
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
        let data_dir = tempdir::TempDir::new("data").unwrap();
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let cluster = Cluster::new(&data_dir, runtime);
            assert!(!cluster.exists());
            assert!(cluster.create().unwrap());
            assert!(cluster.exists());
        }
    }

    #[test]
    fn cluster_create_does_nothing_when_it_already_exists() {
        let data_dir = tempdir::TempDir::new("data").unwrap();
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let cluster = Cluster::new(&data_dir, runtime);
            assert!(!cluster.exists());
            assert!(cluster.create().unwrap());
            assert!(cluster.exists());
            assert!(!cluster.create().unwrap());
        }
    }

    #[test]
    fn cluster_start_stop_starts_and_stops_cluster() {
        let data_dir = tempdir::TempDir::new("data").unwrap();
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
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
        let data_dir = tempdir::TempDir::new("data").unwrap();
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
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
        let data_dir = tempdir::TempDir::new("data").unwrap();
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
            let cluster = Cluster::new(&data_dir, runtime);
            cluster.start().unwrap();
            cluster.connect("template1").unwrap();
            cluster.destroy().unwrap();
        }
    }

    #[test]
    fn cluster_databases_returns_vec_of_database_names() {
        let data_dir = tempdir::TempDir::new("data").unwrap();
        for runtime in Runtime::find_on_path() {
            println!("{:?}", runtime);
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
}
