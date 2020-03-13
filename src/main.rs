#[macro_use]
extern crate clap;

use std::env;
use std::path::PathBuf;
use std::process::exit;

fn main() {
    exit(match parse_args().subcommand() {
        ("shell", Some(args)) => {
            let database_dir: PathBuf = args.value_of("database-dir").unwrap().into();
            let database_name = match args.value_of("database-name") {
                Some(name) => name.into(),
                None => match env::var("PGDATABASE") {
                    Ok(name) => name,
                    Err(_) => "data".into(),
                },
            };
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
                        .help("The directory in which to place, or find, the cluster.")
                        .short("D")
                        .long("datadir")
                        .value_name("DIR")
                        .default_value("db"),
                )
                .arg(
                    Arg::with_name("database-name")
                        .help(concat!(
                    "The database to connect to. The default is taken from the PGDATABASE ",
                    "environment variable. If that is not set, a database named 'data' will ",
                    "be created."))
                        .value_name("PGDATABASE"),
                ),
        )
        .get_matches()
}

fn shell(database_dir: PathBuf, database_name: &str) -> i32 {
    let cluster = postgresfixture::Cluster::new(
        match database_dir.is_absolute() {
            false => env::current_dir().unwrap().join(database_dir),
            true => database_dir,
        },
        postgresfixture::Runtime::default(),
    );
    cluster.start().expect("could not start cluster");
    if !cluster
        .databases()
        .unwrap()
        .contains(&database_name.to_string())
    {
        cluster.createdb(database_name).unwrap();
    }
    cluster.shell(&database_name).expect("shell failed");
    cluster.stop().expect("could not stop cluster");
    0
}
