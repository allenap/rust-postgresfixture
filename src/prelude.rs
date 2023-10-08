//! Basic prelude for `postgresfixture`.

pub use crate::{
    cluster::{self, Cluster, ClusterError},
    coordinate, lock,
    runtime::{self, Runtime, RuntimeError},
    version::{self, Version, VersionError},
};

// Traits.
pub use crate::runtime::strategy::Strategy;
