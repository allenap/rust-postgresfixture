use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Work with ephemeral PostgreSQL clusters.
#[derive(Parser)]
#[clap(author, version, about, long_about = None, propagate_version = true)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start a psql shell, creating and starting the cluster as necessary.
    Shell {
        /// The directory in which to place, or find, the cluster. It will NOT
        /// be destroyed when this command exits.
        #[clap(
            short = 'D',
            long = "datadir",
            env = "PGDATA",
            value_name = "PGDATA",
            default_value = "cluster"
        )]
        database_dir: PathBuf,

        /// The database to connect to.
        #[clap(env = "PGDATABASE", value_name = "PGDATABASE", default_value = "data")]
        database_name: String,
    },
}
