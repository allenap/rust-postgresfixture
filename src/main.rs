mod cli;

use std::fs;
use std::io;
use std::iter;
use std::path::PathBuf;
use std::process::{exit, ExitStatus};

use clap::Parser;
use color_eyre::eyre::{bail, Result, WrapErr};
use color_eyre::{Help, SectionExt};

use postgresfixture::{cluster, coordinate, lock, runtime};

fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = cli::Cli::parse();
    let result = match cli.command {
        cli::Commands::Shell {
            database,
            lifecycle,
        } => run(database.dir, &database.name, lifecycle.destroy, |cluster| {
            check_exit(
                cluster
                    .shell(&database.name)
                    .wrap_err("Starting PostgreSQL shell in cluster failed")?,
            )
        }),
        cli::Commands::Exec {
            database,
            command,
            args,
            lifecycle,
        } => run(database.dir, &database.name, lifecycle.destroy, |cluster| {
            check_exit(
                cluster
                    .exec(&database.name, command, &args)
                    .wrap_err("Executing command in cluster failed")?,
            )
        }),
        cli::Commands::Runtimes => {
            let runtimes_on_path = runtime::Runtime::find_on_path();

            // Get version for each runtime. Throw away errors.
            let mut runtimes: Vec<_> = runtimes_on_path
                .iter()
                .zip(iter::once(true).chain(iter::repeat(false)))
                .filter_map(|(runtime, default)| match runtime.version() {
                    Ok(version) => Some((version, runtime, default)),
                    Err(_) => None,
                })
                .collect();

            // Sort by version. Higher versions will sort last.
            runtimes.sort_by(|(v1, ..), (v2, ..)| v1.cmp(v2));

            for (version, runtime, default) in runtimes {
                let default = if default { "=>" } else { "" };
                match runtime.bindir {
                    Some(ref path) => {
                        println!("{default:2} {version:10} {path}", path = path.display())
                    }
                    None => println!("{default:2} {version:10?} <???>",),
                }
            }
            Ok(0)
        }
    };

    match result {
        Ok(code) => exit(code),
        Err(report) => Err(report),
    }
}

fn check_exit(status: ExitStatus) -> Result<i32> {
    match status.code() {
        Some(code) => Ok(code),
        None => bail!("Command terminated: {status}"),
    }
}

const UUID_NS: uuid::Uuid = uuid::Uuid::from_u128(93875103436633470414348750305797058811);

fn run<F>(database_dir: PathBuf, database_name: &str, destroy: bool, action: F) -> Result<i32>
where
    F: FnOnce(&cluster::Cluster) -> Result<i32> + std::panic::UnwindSafe,
{
    // Create the cluster directory first.
    match fs::create_dir(&database_dir) {
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => (),
        err @ Err(_) => err
            .wrap_err("Could not create database directory")
            .with_section(|| format!("{}", database_dir.display()).header("Database directory:"))?,
        _ => (),
    };

    // Obtain a canonical path to the cluster directory.
    let database_dir = database_dir
        .canonicalize()
        .wrap_err("Could not canonicalize database directory")
        .with_section(|| format!("{}", database_dir.display()).header("Database directory:"))?;

    // Use the canonical path to construct the UUID with which we'll lock this
    // cluster. Use the `Debug` form of `database_dir` for the lock file UUID.
    let lock_uuid = uuid::Uuid::new_v5(&UUID_NS, format!("{:?}", &database_dir).as_bytes());
    let lock = lock::UnlockedFile::try_from(&lock_uuid)
        .wrap_err("Could not create UUID-based lock file")
        .with_section(|| lock_uuid.to_string().header("UUID for lock file:"))?;

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
            .wrap_err("Could not list databases")?
            .contains(&database_name.to_string())
        {
            cluster
                .createdb(database_name)
                .wrap_err("Could not create database")
                .with_section(|| database_name.to_owned().header("Database:"))?;
        }

        // Ignore SIGINT, TERM, and HUP (with ctrlc feature "termination"). The
        // child process will receive the signal, presumably terminate, then
        // we'll tidy up.
        ctrlc::set_handler(|| ()).wrap_err("Could not set signal handler")?;

        // Finally, run the given action.
        action(&cluster)
    })?
}
