//! Safely coordinate use of [`Cluster`].
//!
//! For example, if many concurrent processes want to make use of the same
//! cluster, e.g. as part of a test suite, you can use [`run_and_stop`] to
//! safely start and use the cluster, then stop it when it's no longer needed:
//!
//! ```rust
//! use postgresfixture::prelude::*;
//! let cluster_dir = tempdir::TempDir::new("cluster")?;
//! let data_dir = cluster_dir.path().join("data");
//! let runtime = runtime::strategy::default();
//! let cluster = Cluster::new(&data_dir, runtime)?;
//! let lock_file = cluster_dir.path().join("lock");
//! let lock = lock::UnlockedFile::try_from(lock_file.as_path())?;
//! assert!(coordinate::run_and_stop(&cluster, lock, cluster::exists)?);
//! # Ok::<(), ClusterError>(())
//! ```

use std::time::Duration;

use either::Either::{Left, Right};
use rand::RngCore;

use crate::cluster::{Cluster, ClusterError, State};
use crate::lock;

/// Perform `action` in `cluster`.
///
/// Using the given lock for synchronisation, this creates the cluster if it
/// does not exist, starts it if it's not running, performs the `action`, then
/// (maybe) stops the cluster again, and finally returns the result of `action`.
/// If there are other users of the cluster – i.e. if an exclusive lock cannot
/// be acquired during the shutdown phase – then the cluster is left running.
pub fn run_and_stop<'a, F, T>(
    cluster: &'a Cluster,
    lock: lock::UnlockedFile,
    action: F,
) -> Result<T, ClusterError>
where
    F: std::panic::UnwindSafe + FnOnce(&'a Cluster) -> T,
{
    let lock = startup(cluster, lock)?;
    let action_res = std::panic::catch_unwind(|| action(cluster));
    let _: Option<State> = shutdown(cluster, lock, Cluster::stop)?;
    match action_res {
        Ok(result) => Ok(result),
        Err(err) => std::panic::resume_unwind(err),
    }
}

/// Perform `action` in `cluster`, destroying the cluster before returning.
///
/// Similar to [`run_and_stop`] except this attempts to destroy the cluster
/// – i.e. stop the cluster and completely delete its data directory – before
/// returning. If there are other users of the cluster – i.e. if an exclusive
/// lock cannot be acquired during the shutdown phase – then the cluster is left
/// running and is **not** destroyed.
pub fn run_and_destroy<'a, F, T>(
    cluster: &'a Cluster,
    lock: lock::UnlockedFile,
    action: F,
) -> Result<T, ClusterError>
where
    F: std::panic::UnwindSafe + FnOnce(&'a Cluster) -> T,
{
    let lock = startup(cluster, lock)?;
    let action_res = std::panic::catch_unwind(|| action(cluster));
    let shutdown_res = shutdown(cluster, lock, Cluster::destroy);
    match action_res {
        Ok(result) => shutdown_res.map(|_| result),
        Err(err) => std::panic::resume_unwind(err),
    }
}

fn startup(
    cluster: &Cluster,
    mut lock: lock::UnlockedFile,
) -> Result<lock::LockedFileShared, ClusterError> {
    loop {
        lock = match lock.try_lock_exclusive() {
            Ok(Left(lock)) => {
                // The cluster is locked exclusively by someone/something else.
                // Switch to a shared lock optimistically. This blocks until we
                // get the shared lock.
                let lock = lock.lock_shared()?;
                // The cluster may have been started while that exclusive lock
                // was held, so we must check if the cluster is running now –
                // otherwise we loop back to the top again.
                if cluster.running()? {
                    return Ok(lock);
                }
                // Release all locks then sleep for a random time between 200ms
                // and 1000ms in an attempt to make sure that when there are
                // many competing processes one of them rapidly acquires an
                // exclusive lock and is able to create and start the cluster.
                let lock = lock.unlock()?;
                let delay = rand::thread_rng().next_u32();
                let delay = 200 + (delay % 800);
                let delay = Duration::from_millis(u64::from(delay));
                std::thread::sleep(delay);
                lock
            }
            Ok(Right(lock)) => {
                // We have an exclusive lock, so try to start the cluster.
                cluster.start()?;
                // Once started, downgrade to a shared log.
                return Ok(lock.lock_shared()?);
            }
            Err(err) => return Err(err.into()),
        };
    }
}

fn shutdown<F, T>(
    cluster: &Cluster,
    lock: lock::LockedFileShared,
    action: F,
) -> Result<Option<T>, ClusterError>
where
    F: FnOnce(&Cluster) -> Result<T, ClusterError>,
{
    match lock.try_lock_exclusive() {
        Ok(Left(lock)) => {
            // The cluster is in use by someone/something else. There's nothing
            // more we can do here.
            lock.unlock()?;
            Ok(None)
        }
        Ok(Right(lock)) => {
            // We have an exclusive lock, so we can mutate the cluster.
            match action(cluster) {
                Ok(result) => {
                    lock.unlock()?;
                    Ok(Some(result))
                }
                Err(err) => Err(err),
            }
        }
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        cluster::{Cluster, ClusterError},
        lock::UnlockedFile,
        runtime::{self, Runtime, Strategy},
    };

    use super::{run_and_destroy, run_and_stop};

    type TestResult = Result<(), ClusterError>;

    fn runtimes() -> Box<dyn Iterator<Item = Runtime>> {
        let runtimes = runtime::strategy::default().runtimes().collect::<Vec<_>>();
        Box::new(runtimes.into_iter())
    }

    #[test]
    fn run_and_stop_leaves_the_cluster_in_place() -> TestResult {
        for runtime in runtimes() {
            println!("{runtime:?}");
            let tempdir = tempdir::TempDir::new("somewhere")?;
            let datadir = tempdir.path().join("data");
            let cluster = Cluster::new(&datadir, runtime)?;
            let lockpath = tempdir.path().join("lock");
            let lock = UnlockedFile::try_from(&lockpath)?;
            let databases = run_and_stop(&cluster, lock, Cluster::databases)??;
            assert!(!databases.is_empty());
            assert!(!cluster.running()?);
            assert!(datadir.exists());
        }
        Ok(())
    }

    #[test]
    fn run_and_destroy_removes_the_cluster() -> TestResult {
        for runtime in runtimes() {
            println!("{runtime:?}");
            let tempdir = tempdir::TempDir::new("somewhere")?;
            let datadir = tempdir.path().join("data");
            let cluster = Cluster::new(&datadir, runtime)?;
            let lockpath = tempdir.path().join("lock");
            let lock = UnlockedFile::try_from(&lockpath)?;
            let databases = run_and_destroy(&cluster, lock, Cluster::databases)??;
            assert!(!databases.is_empty());
            assert!(!cluster.running()?);
            assert!(!datadir.exists());
        }
        Ok(())
    }
}
