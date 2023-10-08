use super::{exists, version, Cluster, ClusterError, State::*};
use crate::runtime::{self, strategy::Strategy, Runtime};
use crate::version::{PartialVersion, Version};

use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::str::FromStr;

type TestResult = Result<(), ClusterError>;

fn runtimes() -> Box<dyn Iterator<Item = Runtime>> {
    let runtimes = runtime::strategy::default().runtimes().collect::<Vec<_>>();
    Box::new(runtimes.into_iter())
}

#[test]
fn cluster_new() -> TestResult {
    for runtime in runtimes() {
        println!("{runtime:?}");
        let cluster = Cluster::new("some/path", runtime)?;
        assert_eq!(Path::new("some/path"), cluster.datadir);
        assert!(!cluster.running()?);
    }
    Ok(())
}

#[test]
fn cluster_does_not_exist() -> TestResult {
    for runtime in runtimes() {
        println!("{runtime:?}");
        let cluster = Cluster::new("some/path", runtime)?;
        assert!(!exists(&cluster));
    }
    Ok(())
}

#[test]
fn cluster_does_exist() -> TestResult {
    for runtime in runtimes() {
        println!("{runtime:?}");
        let data_dir = tempdir::TempDir::new("data")?;
        let cluster = Cluster::new(&data_dir, runtime.clone())?;
        cluster.create()?;
        assert!(exists(&cluster));
        let cluster = Cluster::new(&data_dir, runtime)?;
        assert!(exists(&cluster));
    }
    Ok(())
}

#[test]
fn cluster_has_no_version_when_it_does_not_exist() -> TestResult {
    for runtime in runtimes() {
        println!("{runtime:?}");
        let cluster = Cluster::new("some/path", runtime)?;
        assert!(matches!(version(&cluster), Ok(None)));
    }
    Ok(())
}

#[test]
fn cluster_has_version_when_it_does_exist() -> TestResult {
    let data_dir = tempdir::TempDir::new("data")?;
    let version_file = data_dir.path().join("PG_VERSION");
    File::create(&version_file)?;
    for runtime in runtimes() {
        println!("{runtime:?}");
        let pg_version: PartialVersion = runtime.version.into();
        let pg_version = pg_version.widened(); // e.g. 9.6.5 -> 9.6 or 14.3 -> 14.
        std::fs::write(&version_file, format!("{pg_version}\n"))?;
        let cluster = Cluster::new(&data_dir, runtime)?;
        assert!(matches!(version(&cluster), Ok(Some(_))));
    }
    Ok(())
}

#[test]
fn cluster_has_pid_file() -> TestResult {
    let data_dir = PathBuf::from("/some/where");
    for runtime in runtimes() {
        println!("{runtime:?}");
        let cluster = Cluster::new(&data_dir, runtime)?;
        assert_eq!(
            PathBuf::from("/some/where/postmaster.pid"),
            cluster.pidfile()
        );
    }
    Ok(())
}

#[test]
fn cluster_has_log_file() -> TestResult {
    let data_dir = PathBuf::from("/some/where");
    for runtime in runtimes() {
        println!("{runtime:?}");
        let cluster = Cluster::new(&data_dir, runtime)?;
        assert_eq!(
            PathBuf::from("/some/where/postmaster.log"),
            cluster.logfile()
        );
    }
    Ok(())
}

#[test]
fn cluster_create_creates_cluster() -> TestResult {
    for runtime in runtimes() {
        println!("{runtime:?}");
        let data_dir = tempdir::TempDir::new("data")?;
        let cluster = Cluster::new(&data_dir, runtime)?;
        assert!(!exists(&cluster));
        assert!(cluster.create()? == Modified);
        assert!(exists(&cluster));
    }
    Ok(())
}

#[test]
fn cluster_create_creates_cluster_with_neutral_locale_and_timezone() -> TestResult {
    for runtime in runtimes() {
        println!("{runtime:?}");
        let data_dir = tempdir::TempDir::new("data")?;
        let cluster = Cluster::new(&data_dir, runtime.clone())?;
        cluster.start()?;
        let mut conn = cluster.connect("postgres")?;
        let result = conn.query("SHOW ALL", &[])?;
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
        if runtime.version < Version::from_str("9.4.22")? {
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
        if runtime.version >= Version::from_str("16.0")? {
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
            assert_eq!(params.get("lc_messages"), Some(&String::new()));
        } else {
            assert_eq!(params.get("lc_collate"), Some(&"C".into()));
            assert_eq!(params.get("lc_ctype"), Some(&"C".into()));
            assert_eq!(params.get("lc_messages"), Some(&"C".into()));
        }
        assert_eq!(params.get("lc_monetary"), Some(&"C".into()));
        assert_eq!(params.get("lc_numeric"), Some(&"C".into()));
        assert_eq!(params.get("lc_time"), Some(&"C".into()));
        cluster.stop()?;
    }
    Ok(())
}

#[test]
fn cluster_create_does_nothing_when_it_already_exists() -> TestResult {
    for runtime in runtimes() {
        println!("{runtime:?}");
        let data_dir = tempdir::TempDir::new("data")?;
        let cluster = Cluster::new(&data_dir, runtime)?;
        assert!(!exists(&cluster));
        assert!(cluster.create()? == Modified);
        assert!(exists(&cluster));
        assert!(cluster.create()? == Unmodified);
    }
    Ok(())
}

#[test]
fn cluster_start_stop_starts_and_stops_cluster() -> TestResult {
    for runtime in runtimes() {
        println!("{runtime:?}");
        let data_dir = tempdir::TempDir::new("data")?;
        let cluster = Cluster::new(&data_dir, runtime)?;
        cluster.create()?;
        assert!(!cluster.running()?);
        cluster.start()?;
        assert!(cluster.running()?);
        cluster.stop()?;
        assert!(!cluster.running()?);
    }
    Ok(())
}

#[test]
fn cluster_destroy_stops_and_removes_cluster() -> TestResult {
    for runtime in runtimes() {
        println!("{runtime:?}");
        let data_dir = tempdir::TempDir::new("data")?;
        let cluster = Cluster::new(&data_dir, runtime)?;
        cluster.create()?;
        cluster.start()?;
        assert!(exists(&cluster));
        cluster.destroy()?;
        assert!(!exists(&cluster));
    }
    Ok(())
}

#[test]
fn cluster_connect_connects() -> TestResult {
    for runtime in runtimes() {
        println!("{runtime:?}");
        let data_dir = tempdir::TempDir::new("data")?;
        let cluster = Cluster::new(&data_dir, runtime)?;
        cluster.start()?;
        cluster.connect("template1")?;
        cluster.destroy()?;
    }
    Ok(())
}

#[test]
fn cluster_databases_returns_vec_of_database_names() -> TestResult {
    for runtime in runtimes() {
        println!("{runtime:?}");
        let data_dir = tempdir::TempDir::new("data")?;
        let cluster = Cluster::new(&data_dir, runtime)?;
        cluster.start()?;

        let expected: HashSet<String> = ["postgres", "template0", "template1"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let observed: HashSet<String> = cluster.databases()?.iter().cloned().collect();
        assert_eq!(expected, observed);

        cluster.destroy()?;
    }
    Ok(())
}

#[test]
fn cluster_databases_with_non_plain_names_can_be_created_and_dropped() -> TestResult {
    // PostgreSQL identifiers containing hyphens, for example, or where we
    // want to preserve capitalisation, are possible.
    for runtime in runtimes() {
        println!("{runtime:?}");
        let data_dir = tempdir::TempDir::new("data")?;
        let cluster = Cluster::new(&data_dir, runtime)?;
        cluster.start()?;
        cluster.createdb("foo-bar")?;
        cluster.createdb("Foo-BAR")?;

        let expected: HashSet<String> =
            ["foo-bar", "Foo-BAR", "postgres", "template0", "template1"]
                .iter()
                .map(ToString::to_string)
                .collect();
        let observed: HashSet<String> = cluster.databases()?.iter().cloned().collect();
        assert_eq!(expected, observed);

        cluster.dropdb("foo-bar")?;
        cluster.dropdb("Foo-BAR")?;
        cluster.destroy()?;
    }
    Ok(())
}
