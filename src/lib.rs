#![allow(dead_code)]

extern crate semver;

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
    /// Path to the `pg_ctl` executable.
    pg_ctl: PathBuf,
    /// Version number of PostgreSQL.
    ///
    /// https://www.postgresql.org/support/versioning/ shows that
    /// version numbers are essentially SemVer compatible... I think.
    version: Version,
}

impl PostgreSQL {
    pub fn new_with_version<P: AsRef<Path>>(pg_ctl: P, version: Version) -> Self {
        Self{pg_ctl: pg_ctl.as_ref().to_path_buf(), version: version}
    }

    pub fn new<P: AsRef<Path>>(pg_ctl: P) -> Result<Self, VersionError> {
        Ok(Self{
            pg_ctl: pg_ctl.as_ref().to_path_buf(),
            version: get_version(pg_ctl)?,
        })
    }

    pub fn default() -> Result<Self, VersionError> {
        Self::new("pg_ctl")
    }

    pub fn ctl(&self) -> Command {
        let mut command = Command::new(&self.pg_ctl);
        command.env("PATH", path_with_pg_bin(&self.pg_ctl));
        command
    }
}

fn path_with_pg_bin(pg_ctl: &Path) -> &'static str {
    "fred"
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

    pub fn exists(&self) -> bool {
        self.datadir.is_dir() && self.datadir.join("PG_VERSION").is_file()
    }

    pub fn is_running(&self) -> bool {
        false
    }

    // fn execute(&self) -> Result<()> {
    //     // env = options.pop("env", environ).copy()
    //     // env["PATH"] = path_with_pg_bin(env.get("PATH", ""), self.version)
    //     // env["PGDATA"] = env["PGHOST"] = self.datadir
    //     // check_call(command, env=env, **options)
    //
    // }

}


#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::Cluster;
    use super::PostgreSQL;

    use semver::Version;
    use std::fs::File;
    use std::path::Path;

    #[test]
    fn postgres_new_discovers_version() {
        let pg = PostgreSQL::new("pg_ctl").unwrap();
        assert!(pg.version.major >= 9);
    }

    #[test]
    fn postgres_default_discovers_version() {
        let pg = PostgreSQL::default().unwrap();
        assert!(pg.version.major >= 9);
    }

    #[test]
    fn create_new_cluster() {
        let pg = PostgreSQL::new_with_version(
            "pg_ctl", Version::parse("1.2.3").unwrap());
        let cluster = Cluster::new("some/path", pg);
        assert_eq!(Path::new("some/path"), cluster.datadir);
        // assert_eq!((1, 2, 3), (
        //     cluster.version.major, cluster.version.minor,
        //     cluster.version.patch));
    }

    #[test]
    fn cluster_does_not_exist() {
        let pg = PostgreSQL::new_with_version(
            "pg_ctl", Version::parse("1.2.3").unwrap());
        let cluster = Cluster::new("some/path", pg);
        assert!(!cluster.exists());
    }

    #[test]
    fn cluster_does_exist() {
        let data_dir = tempdir::TempDir::new("data").unwrap();
        let version_file = data_dir.path().join("PG_VERSION");
        File::create(&version_file).unwrap();
        let pg = PostgreSQL::new_with_version(
            "pg_ctl", Version::parse("1.2.3").unwrap());
        let cluster = Cluster::new(&data_dir, pg);
        assert!(cluster.exists());
    }
}
