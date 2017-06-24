#![allow(dead_code)]

extern crate semver;

use std::env;
use std::ffi;
use std::io;
use std::process::Command;
use std::path::{Path,PathBuf};
use semver::{Version,SemVerError};


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
                if let Some(path) = prepend_path(&bindir).unwrap() {
                    command.env("PATH", path);
                }
            },
            None => {
                command = Command::new("pg_ctl");
            }
        }
        command
    }
}


fn prepend_path(bindir: &Path)
        -> Result<Option<ffi::OsString>, env::JoinPathsError> {
    Ok(match env::var_os("PATH") {
        None => None,
        Some(path) => {
            let mut paths = vec!(bindir.to_path_buf());
            paths.extend(
                env::split_paths(&path)
                    .filter(|path| path != bindir));
            Some(env::join_paths(paths)?)
        },
    })
}


struct Cluster {
    /// The data directory of the cluster.
    ///
    /// Corresponds to the `PGDATA` environment variable.
    datadir: PathBuf,
    /// The installation of PostgreSQL to use with this cluster.
    postgres: PostgreSQL,
}

impl Cluster {

    pub fn new<P: AsRef<Path>>(datadir: P, postgres: PostgreSQL) -> Self {
        Cluster{
            datadir: datadir.as_ref().to_path_buf(),
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

    pub fn is_running(&self) -> io::Result<bool> {
        self.ctl().arg("status").output()
            .map(|output| output.status.success())
        // TODO: Success depends on version.
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
        env::split_paths(&env::var_os("PATH").unwrap())
            .find(|path| path.join("pg_ctl").exists()).unwrap()
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
    fn create_new_cluster() {
        let pg = PostgreSQL{bindir: None, version: "1.2.3".parse().unwrap()};
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
}
