# rust-postgresfixture

A [Rust](https://www.rust-lang.org/) library and command-line tool for creating
standalone PostgreSQL clusters and databases, useful for experimentation,
development, and testing.

It's based on the Python [postgresfixture][] library which saw heavy use in
[MAAS](https://maas.io/). That was (and is) a useful tool when experimenting
with PostgreSQL. For example we could use it to bring up a cluster to run a
development server. However, it came into its own in MAAS's test suites, and was
key to [making MAAS's test suites faster][maas-faster-tests].

[postgresfixture]: https://pypi.python.org/pypi/postgresfixture
[maas-faster-tests]: https://allenap.me/post/the-way-to-run-tests-quickly-in-maas

This Rust version started out as a straightforward port but it has deviated
significantly from the design of its Python counterpart.

This code works and seems to be reliable, but the command-line and API may
change before 1.0, potentially causing breakage. If this is a problem I suggest
pinning on a specific version and checking back once in a while to see if it can
be upgraded, or use something automated like [Dependabot][dependabot].

[dependabot]: https://github.com/dependabot

## Command-line utility

After [installing Cargo][install-cargo], `cargo install postgresfixture` will
install a `postgresfixture` binary in `~/.cargo/bin`, which the Cargo
installation process will probably have added to your `PATH`.

**Note** that this tool does _not_ come with any PostgreSQL runtimes. You must
install these yourself and add their `bin` directories to `PATH`. To select a
specific runtime you must set `PATH` such that the runtime you want to use is
before any others. The `runtimes` subcommand can show you what is available and
what runtime will actually be used.

```shellsession
$ postgresfixture --help
Easily create and manage PostgreSQL clusters on demand for testing and development.

Usage: postgresfixture <COMMAND>

Commands:
  shell     Start a psql shell, creating and starting the cluster as necessary
  exec      Execute an arbitrary command, creating and starting the cluster as necessary
  runtimes  List discovered PostgreSQL runtimes
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version

$ postgresfixture runtimes
   10.22      /opt/homebrew/Cellar/postgresql@10/10.22_6/bin
   11.21      /opt/homebrew/Cellar/postgresql@11/11.21/bin
   12.16      /opt/homebrew/Cellar/postgresql@12/12.16/bin
   13.12      /opt/homebrew/Cellar/postgresql@13/13.12/bin
   14.9       /opt/homebrew/Cellar/postgresql@14/14.9/bin
   15.4       /opt/homebrew/Cellar/postgresql@15/15.4/bin
=> 16.0       /opt/homebrew/bin

$ postgresfixture shell
postgres=# select …

$ postgresfixture exec pg_dump
--
-- PostgreSQL database dump
--
…
```

## Use as a library

The essential functionality in this crate is in the `Cluster` struct and its
implementation. This covers the logic you need to create, run, and destroy
PostgreSQL clusters of any officially supported version (and a few older
versions that are not supported upstream).

```rust
use postgresfixture::prelude::*;
for runtime in strategy::default().runtimes() {
  let data_dir = tempdir::TempDir::new("data")?;
  let cluster = Cluster::new(&data_dir, runtime)?;
  cluster.start()?;
  assert_eq!(cluster.databases()?, vec!["postgres", "template0", "template1"]);
  let mut conn = cluster.connect("template1")?;
  let rows = conn.query("SELECT 1234 -- …", &[])?;
  let collations: Vec<i32> = rows.iter().map(|row| row.get(0)).collect();
  assert_eq!(collations, vec![1234]);
  cluster.stop()?;
}
# Ok::<(), cluster::Error>(())
```

You may want to use this with the functions in the `coordinate` module like
`run_and_stop` and `run_and_destroy`. These add locking to the setup and
teardown steps of using a cluster so that multiple processes can safely share a
single on-demand cluster.

## Contributing

If you feel the urge to hack on this code, here's
how to get started:

- [Install cargo][install-cargo],
- Clone this repository,
- Build it: `cargo build`.

[install-cargo]: https://crates.io/install

### Running the tests

After installing the source (see above) run tests with: `cargo test`.

**However**, it's important to test against multiple versions of PostgreSQL. The
tests will look for all PostgreSQL runtimes on `PATH` and run tests for all of
them.

First you must install multiple versions of PostgreSQL on your machine. Read on
for platform-specific notes. Once you've installed the versions you want,
[`postgresfixture::runtime::strategy::default()`] may be able to automatically
find them – and, since this function is used by tests, those runtimes will
automatically be tested.

#### Debian & Ubuntu

From https://wiki.postgresql.org/wiki/Apt:

```shellsession
$ sudo apt-get install -y postgresql-common
$ sudo sh /usr/share/postgresql-common/pgdg/apt.postgresql.org.sh -y
$ sudo apt-get install -y postgresql-{9.{4,5,6},10,11,12,13}  # Adjust as necessary.
```

#### macOS

Using [Homebrew](https://brew.sh/):

```shellsession
$ brew install postgresql  # Latest version.
$ brew install postgresql@{9.{4,5,6},10,11,12,13}  # Adjust as necessary.
```

### Making a release

1. Bump version in [`Cargo.toml`](Cargo.toml).
2. Paste updated `--help` output into [`README.md`](README.md) (this file; see
   near the top). On macOS the command `cargo run -- --help | pbcopy` is
   helpful.
3. Build **and** test: `cargo build && cargo test`. The latter on its own does
   do a build, but a test build can hide warnings about dead code, so do both.
4. Commit with message "Bump version to `$VERSION`."
5. Tag with "v`$VERSION`", e.g. `git tag v1.0.10`.
6. Push: `git push && git push --tags`.
7. Publish: `cargo publish`.

## License

This project is licensed under the Apache 2.0 License. See the
[LICENSE](LICENSE) file for details.
