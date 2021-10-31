# rust-postgresfixture

A [Rust](https://www.rust-lang.org/) library and command-line tool for creating
short-lived PostgreSQL clusters and databases, especially useful for testing and
development.

It's based on the Python [postgresfixture][] library which saw heavy use in
[MAAS](https://maas.io/). That was (and is) a useful tool when experimenting
with PostgreSQL. For example we could use it to bring up a cluster to run a
development server, but it came into its own in MAAS's test suites, and was key
to [making MAAS's test suites faster][maas-faster-tests].

[postgresfixture]: https://pypi.python.org/pypi/postgresfixture
[maas-faster-tests]: https://allenap.me/post/the-way-to-run-tests-quickly-in-maas/

This started out as a straightforward port, but it's starting to deviate from
the design of its Python counterpart. I'm not sure exactly where it will end up
yet, but it will at least support the same use cases, albeit in its own way.

## Command-line utility

If you have [installed Cargo][install-cargo], you can install
rust-postgresfixture with `cargo install postgresfixture`. This puts a
`postgresfixture` binary in `~/.cargo/bin`, which the Cargo installation process
will probably have added to your `PATH`.

```shellsession
$ postgresfixture --help
rust-postgresfixture 0.2.4
Gavin Panella <gavinpanella@gmail.com>
Work with ephemeral PostgreSQL clusters.

USAGE:
    postgresfixture <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    shell    Start a psql shell, creating and starting the cluster as necessary.

Based on the Python postgresfixture library <https://pypi.python.org/pypi/postgresfixture>.

$ postgresfixture shell
data=# select
untaunting-paxton

$ petname -s _ -w 3
suitably_overdelicate_jamee
```

## Use as a library

The highest level functionality in this create is all in the `Cluster` struct
and its implementation. This covers all the logic you need to safely create,
run, and destroy PostgreSQL clusters of any officially supported version (and a
few older versions that are not supported upstream).

```rust
let data_dir = tempdir::TempDir::new("data").unwrap();
let runtime = postgresfixture::Runtime::default();
let cluster = postgresfixture::Cluster::new(&data_dir, runtime);
```

## Contributing

This code is **beta** quality. Some things will likely change, like the [locking
scheme](https://github.com/allenap/rust-postgresfixture/issues/34), before
version 1.0, but that's unlikely to cause API breakage. If you feel the urge to
hack on this code, here's how to get started:

- [Install cargo][install-cargo],
- Clone this repository,
- Build it: `cargo build`.

[install-cargo]: https://crates.io/install

### Running the tests

After installing the source (see above) run tests with: `cargo test`.

**However**, it's important to test against multiple versions of PostgreSQL. The
tests will look for all PostgreSQL runtimes on `PATH` and run tests for all of
them. To make this easier, the [`test`](test) script will help find the runtimes
and add them to `PATH`, but first you must install muliple versions of
PostgreSQL on your machine. Read on for platform-specific notes.

#### Debian & Ubuntu

From https://wiki.postgresql.org/wiki/Apt:

```shellsession
$ sudo apt-get install -y postgresql-common
$ sudo sh /usr/share/postgresql-common/pgdg/apt.postgresql.org.sh -y
```

#### macOS

Using [Homebrew](https://brew.sh/):

```shellsession
$ brew install postgresql  # Latest version.
$ brew install postgresql@{9.{4,5,6},10,11,12,13}  # Others; adjust as necessary.
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
6. Push: `git push --tags`.
7. Publish: `cargo publish`.

## License

This project is licensed under the Apache 2.0 License. See the
[LICENSE](LICENSE) file for details.
