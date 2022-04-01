use either::Either::{Left, Right};

use crate::lock;
use crate::{Cluster, ClusterError};

pub fn run<F, T>(
    cluster: &Cluster,
    mut lock: lock::UnlockedFile,
    action: F,
) -> Result<T, ClusterError>
where
    F: FnOnce() -> T,
{
    let delay = std::time::Duration::from_millis(1111);

    let lock: lock::LockedFileShared = loop {
        lock = match lock.try_lock_exclusive() {
            Ok(Left(lock)) => {
                // The cluster is locked exclusively. Switch to a shared
                // lock optimistically.
                let lock = lock.lock_shared()?;
                // The cluster may have been stopped while held in that
                // exclusive lock, so we must check if the cluster is
                // running _now_, else loop back to the top again.
                if cluster.running()? {
                    break lock;
                } else {
                    let lock = lock.unlock()?;
                    std::thread::sleep(delay);
                    lock
                }
            }
            Ok(Right(lock)) => {
                // We have an exclusive lock, so try to start the cluster.
                cluster.start()?;
                // Once started, downgrade to a shared log.
                break lock.lock_shared()?;
            }
            Err(err) => return Err(err.into()),
        };
    };

    let result = action();

    // If we can get an exclusive lock, then we're the only
    // remaining user, and we should shut down the cluster.
    match lock.try_lock_exclusive() {
        Ok(Left(lock)) => lock.unlock()?,
        Ok(Right(lock)) => match cluster.stop() {
            Ok(_) | Err(ClusterError::InUse) => lock.unlock()?,
            Err(err) => return Err(err),
        },
        Err(err) => return Err(err.into()),
    };

    Ok(result)
}
