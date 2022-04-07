mod cli;

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::exit;

use clap::Parser;

use postgresfixture::{cluster, coordinate, lock, runtime};

fn main() {
    let cli = cli::Cli::parse();
    exit(match cli.command {
        cli::Commands::Shell {
            database,
            lifecycle,
        } => shell(database.dir, &database.name, lifecycle.destroy),
        cli::Commands::Exec {
            database,
            command,
            args,
            lifecycle,
        } => exec(
            database.dir,
            &database.name,
            command,
            &args,
            lifecycle.destroy,
        ),
    });
}

const UUID_NS: uuid::Uuid = uuid::Uuid::from_u128(93875103436633470414348750305797058811);

fn shell(database_dir: PathBuf, database_name: &str, destroy: bool) -> i32 {
    // Create the cluster directory first.
    match fs::create_dir(&database_dir) {
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => (),
        other => other.expect("could not create database directory"),
    };

    // Obtain a canonical path to the cluster directory.
    let database_dir = database_dir
        .canonicalize()
        .expect("could not canonicalize database directory");

    // Use the canonical path to construct the UUID with which we'll lock this
    // cluster. Use the `Debug` form of `database_dir` for the lock file UUID.
    let lock_uuid = uuid::Uuid::new_v5(&UUID_NS, format!("{:?}", &database_dir).as_bytes());
    let lock = lock::UnlockedFile::try_from(&lock_uuid).expect("could not create lock file");

    // For now use the default PostgreSQL runtime.
    let runtime = runtime::Runtime::default();
    let cluster = cluster::Cluster::new(&database_dir, runtime);

    let runner = if destroy {
        coordinate::run_and_destroy
    } else {
        coordinate::run_and_stop
    };

    runner(&cluster, lock, || {
        if !cluster
            .databases()
            .expect("could not list databases")
            .contains(&database_name.to_string())
        {
            cluster
                .createdb(database_name)
                .expect("could not create database");
        }
        // Ignore SIGINT, TERM, and HUP (with ctrlc feature "termination"). The
        // child process will receive the signal, presumably terminate, then
        // we'll tidy up.
        ctrlc::set_handler(|| ()).expect("could not set signal handler");
        cluster.shell(database_name).expect("shell failed");
    })
    .unwrap();

    0
}

fn exec<T: AsRef<OsStr>>(
    database_dir: PathBuf,
    database_name: &str,
    command: T,
    args: &[T],
    destroy: bool,
) -> i32 {
    // Create the cluster directory first.
    match fs::create_dir(&database_dir) {
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => (),
        other => other.expect("could not create database directory"),
    };

    // Obtain a canonical path to the cluster directory.
    let database_dir = database_dir
        .canonicalize()
        .expect("could not canonicalize database directory");

    // Use the canonical path to construct the UUID with which we'll lock this
    // cluster. Use the `Debug` form of `database_dir` for the lock file UUID.
    let lock_uuid = uuid::Uuid::new_v5(&UUID_NS, format!("{:?}", &database_dir).as_bytes());
    let lock = lock::UnlockedFile::try_from(&lock_uuid).expect("could not create lock file");

    // For now use the default PostgreSQL runtime.
    let runtime = runtime::Runtime::default();
    let cluster = cluster::Cluster::new(&database_dir, runtime);

    let runner = if destroy {
        coordinate::run_and_destroy
    } else {
        coordinate::run_and_stop
    };

    runner(&cluster, lock, || {
        if !cluster
            .databases()
            .expect("could not list databases")
            .contains(&database_name.to_string())
        {
            cluster
                .createdb(database_name)
                .expect("could not create database");
        }
        // Ignore SIGINT, TERM, and HUP (with ctrlc feature "termination"). The
        // child process will receive the signal, presumably terminate, then
        // we'll tidy up.
        ctrlc::set_handler(|| ()).expect("could not set signal handler");
        cluster
            .exec(database_name, command, args)
            .expect("exec failed");
    })
    .unwrap();

    0
}
