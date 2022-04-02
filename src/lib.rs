//!
//! The highest level functionality in this create is all in
//! [`cluster::Cluster`]. This covers all the logic you need to safely create,
//! run, and destroy PostgreSQL clusters of any officially supported version
//! (and a few older versions that are not supported upstream).
//!
//! ```rust
//! let data_dir = tempdir::TempDir::new("data").unwrap();
//! # use postgresfixture::{cluster, runtime};
//! let runtime = runtime::Runtime::default();
//! let cluster = cluster::Cluster::new(&data_dir, runtime);
//! ```
//!

pub mod cluster;
pub mod coordinate;
pub mod lock;
pub mod runtime;
pub mod version;

mod util;
