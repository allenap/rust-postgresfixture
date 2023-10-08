mod cli;

use std::fs;
use std::io;
use std::iter;
use std::path::PathBuf;
use std::process::{exit, ExitStatus};

use clap::Parser;
use color_eyre::eyre::{bail, Result, WrapErr};
use color_eyre::{Help, SectionExt};

use postgresfixture::{cluster, coordinate, lock, runtime, runtime::strategy::RuntimeStrategy};

fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = cli::Cli::parse();
    let result = match cli.command {
        cli::Commands::Shell {
            cluster,
            database,
            lifecycle,
        } => run(
            cluster.dir,
            &database.name,
            lifecycle.destroy,
            initialise(cluster.mode),
            |cluster| {
                check_exit(
                    cluster
                        .shell(&database.name)
                        .wrap_err("Starting PostgreSQL shell in cluster failed")?,
                )
            },
        ),
        cli::Commands::Exec {
            cluster,
            database,
            command,
            args,
            lifecycle,
        } => run(
            cluster.dir,
            &database.name,
            lifecycle.destroy,
            initialise(cluster.mode),
            |cluster| {
                check_exit(
                    cluster
                        .exec(&database.name, command, &args)
                        .wrap_err("Executing command in cluster failed")?,
                )
            },
        ),
        cli::Commands::Runtimes { platform } => {
            let runtimes_found = {
                let mut runtimes: Vec<_> =
                    runtime::strategy::RuntimesOnPath::Env.runtimes().collect();
                if platform {
                    runtimes.extend(runtime::strategy::RuntimesOnPlatform.runtimes());
                }
                runtimes
            };

            // Get version for each runtime. Throw away errors.
            let mut runtimes: Vec<_> = runtimes_found
                .into_iter()
                .zip(iter::once(true).chain(iter::repeat(false)))
                .collect();

            // Sort by version. Higher versions will sort last.
            runtimes.sort_by(|(ra, ..), (rb, ..)| ra.version.cmp(&rb.version));

            for (runtime, default) in runtimes {
                let default = if default { "=>" } else { "" };
                println!(
                    "{default:2} {version:10} {bindir}",
                    bindir = runtime.bindir.display(),
                    version = runtime.version,
                )
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

fn run<INIT, ACTION>(
    database_dir: PathBuf,
    database_name: &str,
    destroy: bool,
    initialise: INIT,
    action: ACTION,
) -> Result<i32>
where
    INIT: std::panic::UnwindSafe + FnOnce(&cluster::Cluster) -> Result<(), cluster::ClusterError>,
    ACTION: FnOnce(&cluster::Cluster) -> Result<i32> + std::panic::UnwindSafe,
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

    let strategy = runtime::strategy::RuntimeStrategySet::default();
    let cluster = cluster::Cluster::new(&database_dir, strategy)?;

    let runner = if destroy {
        coordinate::run_and_destroy
    } else {
        coordinate::run_and_stop
    };

    runner(&cluster, lock, |cluster: &cluster::Cluster| {
        initialise(cluster)?;

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
        action(cluster)
    })?
}

/// Create an initialisation function that will set appropriate PostgreSQL
/// settings, e.g. `fsync`, `full_page_writes`, etc. that need to be set early.
fn initialise(
    mode: Option<cli::Mode>,
) -> impl std::panic::UnwindSafe + FnOnce(&cluster::Cluster) -> Result<(), cluster::ClusterError> {
    match mode {
        Some(cli::Mode::Fast) => {
            |cluster: &cluster::Cluster| {
                let mut conn = cluster.connect("template1")?;
                conn.execute("ALTER SYSTEM SET fsync = 'off'", &[])?;
                conn.execute("ALTER SYSTEM SET full_page_writes = 'off'", &[])?;
                conn.execute("ALTER SYSTEM SET synchronous_commit = 'off'", &[])?;
                // TODO: Check `pg_file_settings` for errors before reloading.
                conn.execute("SELECT pg_reload_conf()", &[])?;
                Ok(())
            }
        }
        Some(cli::Mode::Slow) => {
            |cluster: &cluster::Cluster| {
                let mut conn = cluster.connect("template1")?;
                conn.execute("ALTER SYSTEM RESET fsync", &[])?;
                conn.execute("ALTER SYSTEM RESET full_page_writes", &[])?;
                conn.execute("ALTER SYSTEM RESET synchronous_commit", &[])?;
                // TODO: Check `pg_file_settings` for errors before reloading.
                conn.execute("SELECT pg_reload_conf()", &[])?;
                Ok(())
            }
        }
        None => |_: &cluster::Cluster| Ok(()),
    }
}
