#![warn(clippy::pedantic)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]

//!
//! The essential functionality in this crate is in the `Cluster` struct and its
//! implementation. This covers the logic you need to create, run, and destroy
//! PostgreSQL clusters of any officially supported version (and a few older
//! versions that are not supported upstream).
//!
//! ```rust
//! use postgresfixture::prelude::*;
//! for runtime in strategy::default().runtimes() {
//!   let data_dir = tempdir::TempDir::new("data")?;
//!   let cluster = Cluster::new(&data_dir, runtime)?;
//!   cluster.start()?;
//!   assert_eq!(cluster.databases()?, vec!["postgres", "template0", "template1"]);
//!   let mut conn = cluster.connect("template1")?;
//!   let rows = conn.query("SELECT 1234 -- …", &[])?;
//!   let collations: Vec<i32> = rows.iter().map(|row| row.get(0)).collect();
//!   assert_eq!(collations, vec![1234]);
//!   cluster.stop()?;
//! }
//! # Ok::<(), cluster::Error>(())
//! ```
//!
//! You may want to use this with the functions in the [`coordinate`] module
//! like [`run_and_stop`][coordinate::run_and_stop] and
//! [`run_and_destroy`][coordinate::run_and_destroy]. These add locking to the
//! setup and teardown steps of using a cluster so that multiple processes can
//! safely share a single on-demand cluster.
//!

#[macro_use]
extern crate lazy_static;

#[doc = include_str!("../README.md")]
#[cfg(doctest)]
pub struct README;

pub mod cluster;
pub mod coordinate;
pub mod lock;
pub mod prelude;
pub mod runtime;
pub mod version;

mod util;
