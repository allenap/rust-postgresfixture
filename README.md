# rust-postgresfixture

A library and command-line tool for creating short-lived PostgreSQL clusters and
databases in [Rust](https://www.rust-lang.org/).

It's based on the Python [postgresfixture][] library which saw heavy use in
[MAAS](https://maas.io/). That was (and is) a useful tool when experimenting
with PostgreSQL, or to bring up a cluster to run MAAS in development, but it
came into its own in MAAS's test suites. It was one of the cornerstones to my
efforts to [make MAAS's test suites faster][maas-faster-tests].

[postgresfixture]: https://pypi.python.org/pypi/postgresfixture
[maas-faster-tests]: https://allenap.me/post/the-way-to-run-tests-quickly-in-maas/

This started out as a straightforward port, but it's starting to deviate from
the design of its Python counterpart. I'm not sure exactly where it will end up
yet, but it will at least support the same use cases, albeit in its own way.


## Getting Started

This code is **alpha** and is some way from feature parity with its Python
ancestor. If you feel the urge to hack on this code, here's how to get started:

  * [Install cargo](https://crates.io/install),
  * Clone this repository,
  * Build it: `cargo build`.


## Running the tests

After installing the source (see above) run tests with: `cargo test`.


## License

This project is licensed under the Apache 2.0 License. See the
[LICENSE](LICENSE) file for details.
