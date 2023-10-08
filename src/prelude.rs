//! Basic prelude for `postgresfixture`.

pub use crate::{
    cluster::{self, Cluster},
    coordinate, lock,
    runtime::{self, strategy::Strategy, Runtime},
    version::{self, Version},
};
