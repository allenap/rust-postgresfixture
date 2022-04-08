use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Work with ephemeral PostgreSQL clusters.
#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start a psql shell, creating and starting the cluster as necessary.
    #[clap(display_order = 1)]
    Shell {
        #[clap(flatten)]
        database: DatabaseArgs,

        #[clap(flatten)]
        lifecycle: LifecycleArgs,
    },

    /// Execute an arbitrary command, creating and starting the cluster as
    /// necessary.
    #[clap(display_order = 2)]
    Exec {
        #[clap(flatten)]
        database: DatabaseArgs,

        #[clap(flatten)]
        lifecycle: LifecycleArgs,

        /// The executable to invoke. By default it will start a shell.
        #[clap(env = "SHELL", value_name = "COMMAND")]
        command: OsString,

        /// Arguments to pass to the executable.
        #[clap(value_name = "ARGUMENTS")]
        args: Vec<OsString>,
    },

    /// List PostgreSQL runtimes discovered on PATH.
    #[clap(display_order = 3)]
    Runtimes,
}

#[derive(Args)]
pub struct DatabaseArgs {
    /// The directory in which to place, or find, the cluster.
    #[clap(
        short = 'D',
        long = "datadir",
        env = "PGDATA",
        value_name = "PGDATA",
        default_value = "cluster",
        display_order = 1
    )]
    pub dir: PathBuf,

    /// The database to connect to.
    #[clap(
        short = 'd',
        long = "database",
        env = "PGDATABASE",
        value_name = "PGDATABASE",
        default_value = "data",
        display_order = 2
    )]
    pub name: String,
}
#[derive(Args)]
pub struct LifecycleArgs {
    /// Destroy the cluster after use. WARNING: This will DELETE THE DATA
    /// DIRECTORY. The default is to NOT destroy the cluster.
    #[clap(long = "destroy", display_order = 100)]
    pub destroy: bool,
}
