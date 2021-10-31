//!
//! The highest level functionality in this create is all in [`Cluster`]. This
//! covers all the logic you need to safely create, run, and destroy PostgreSQL
//! clusters of any officially supported version (and a few older versions that
//! are not supported upstream).
//!
//! ```rust
//! let data_dir = tempdir::TempDir::new("data").unwrap();
//! let runtime = postgresfixture::Runtime::default();
//! let cluster = postgresfixture::Cluster::new(&data_dir, runtime);
//! ```
//!

mod cluster;
mod lock;
mod runtime;
mod util;
mod version;

pub use cluster::{Cluster, ClusterError};
pub use runtime::Runtime;
