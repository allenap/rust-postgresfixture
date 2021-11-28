#[macro_use]
extern crate clap;

use std::env;
use std::path::PathBuf;
use std::process::exit;

use postgresfixture::ClusterError;

fn main() {
    exit(match parse_args().subcommand() {
        ("shell", Some(args)) => {
            let database_dir: PathBuf = args.value_of("database-dir").unwrap().into();
            let database_name: String = args.value_of("database-name").unwrap().into();
            shell(database_dir, &database_name)
        }
        (command, _) => {
            unreachable!("subcommand branch for {:?} is missing", command);
        }
    });
}

/// Parse command-line arguments.
fn parse_args() -> clap::ArgMatches<'static> {
    use clap::{App, AppSettings, Arg, SubCommand};
    App::new("rust-postgresfixture")
        .setting(AppSettings::DisableHelpSubcommand)
        .setting(AppSettings::SubcommandRequired)
        .setting(AppSettings::VersionlessSubcommands)
        .version(crate_version!())
        .author(crate_authors!())
        .about("Work with ephemeral PostgreSQL clusters.")
        .after_help(concat!(
            "Based on the Python postgresfixture library ",
            "<https://pypi.python.org/pypi/postgresfixture>."
        ))
        .subcommand(
            SubCommand::with_name("shell")
                .about("Start a psql shell, creating and starting the cluster as necessary.")
                .arg(
                    Arg::with_name("database-dir")
                        .help(concat!(
                            "The directory in which to place, or find, the cluster. The default ",
                            "is taken from the PGDATA environment variable. If that is not set, ",
                            "the cluster will be created in a directory named 'cluster'."))
                        .short("D")
                        .long("datadir")
                        .value_name("PGDATA")
                        .default_value("cluster")
                        .env("PGDATA"),
                )
                .arg(
                    Arg::with_name("database-name")
                        .help(concat!(
                            "The database to connect to. The default is taken from the PGDATABASE ",
                            "environment variable. If that is not set, a database named 'data' will ",
                            "be created."))
                        .value_name("PGDATABASE")
                        .default_value("data")
                        .env("PGDATABASE"),
                ),
        )
        .get_matches()
}

fn shell(database_dir: PathBuf, database_name: &str) -> i32 {
    let cluster = postgresfixture::Cluster::new(
        match database_dir.is_absolute() {
            true => database_dir,
            false => env::current_dir()
                .expect("could not get current working directory")
                .join(database_dir),
        },
        postgresfixture::Runtime::default(),
    );
    match cluster.start() {
        Err(ClusterError::InUse) => false,
        other => other.expect("could not start cluster"),
    };
    if !cluster
        .databases()
        .expect("could not list databases")
        .contains(&database_name.to_string())
    {
        cluster
            .createdb(database_name)
            .expect("could not create database");
    }
    cluster.shell(database_name).expect("shell failed");
    match cluster.stop() {
        Err(ClusterError::InUse) => false,
        other => other.expect("could not stop cluster"),
    };
    0
}
