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
        cluster: ClusterArgs,

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
        cluster: ClusterArgs,

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

    /// List discovered PostgreSQL runtimes.
    ///
    /// The runtime shown on the line beginning with `=>` is the default.
    #[clap(display_order = 3)]
    Runtimes {
        /// Find runtimes using platform-specific logic too.
        ///
        /// Without this option, only `PATH` is searched.
        #[clap(long = "platform", default_value_t = false)]
        platform: bool,
    },
}

#[derive(Args)]
pub struct ClusterArgs {
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

    /// Run the cluster in a "faster" mode.
    ///
    /// This disables `fsync`, `full_page_writes`, and `synchronous_commit` in
    /// the cluster. This can make the cluster faster but it can also lead to
    /// unrecoverable data corruption in the event of a power failure or system
    /// crash. Useful for tests, for example, but probably not production.
    ///
    /// In the future this may make additional or different changes. See
    /// https://www.postgresql.org/docs/16/runtime-config-wal.html for more
    /// information.
    ///
    /// This option is STICKY. Once you've used it, the cluster will be
    /// configured to be "faster but less safe" and you do not need to specify
    /// it again. To find out if the cluster is running in this mode, open a
    /// `psql` shell (e.g. `postgresfixture shell`) and run `SHOW fsync; SHOW
    /// full_page_writes; SHOW synchronous_commit;`.
    #[clap(long = "faster-but-less-safe", action = clap::ArgAction::SetTrue, default_value_t = false, display_order = 2)]
    pub faster: bool,

    /// Run the cluster in a "safer" mode.
    ///
    /// This is the opposite of `--faster-but-less-safe`, i.e. it runs with
    /// `fsync`, `full_page_writes`, and `synchronous_commit` enabled in the
    /// cluster.
    ///
    /// NOTE: this actually *resets* the `fsync`, `full_page_writes`, and
    /// `synchronous_commit` settings to their defaults. Unless they've been
    /// configured differently in the cluster's `postgresql.conf`, the default
    /// for these settings is on/enabled.
    ///
    /// This option is STICKY. Once you've used it, the cluster will be
    /// configured to be "slower and safer" and you do not need to specify it
    /// again. To find out if the cluster is running in this mode, open a `psql`
    /// shell (e.g. `postgresfixture shell`) and run `SHOW fsync; SHOW
    /// full_page_writes; SHOW synchronous_commit;`.
    #[clap(long = "slower-but-safer", action = clap::ArgAction::SetTrue, default_value_t = false, display_order = 3, conflicts_with = "faster")]
    pub slower: bool,
}

#[derive(Args)]
pub struct DatabaseArgs {
    /// The database to connect to.
    #[clap(
        short = 'd',
        long = "database",
        env = "PGDATABASE",
        value_name = "PGDATABASE",
        default_value = "postgres",
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
