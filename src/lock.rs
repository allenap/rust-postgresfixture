use std::fs::File;
use std::os::unix::io::AsRawFd;

use nix::fcntl::{flock, FlockArg};
use nix::Result;

/// Call closures with exclusive or shared locks on _self_, whatever that might mean in context of
/// the implementation. For example, implementing this for `File` might imply taking advantage of
/// the platform's advisory locking facilities.
pub trait LockDo {
    /// Call the given closure with an exclusive lock on _self_.
    ///
    /// If _self_ has already been locked elsewhere, this will block until it is able to acquire an
    /// exclusive lock.
    fn do_exclusive<F, T>(&self, action: F) -> Result<T>
    where
        F: FnOnce() -> T;

    /// Call the given closure with a shared lock on _self_.
    ///
    /// If _self_ has already been locked elsewhere, this will block until it is able to acquire a
    /// shared lock.
    fn do_shared<F, T>(&self, action: F) -> Result<T>
    where
        F: FnOnce() -> T;
}

impl LockDo for File {
    /// Call the given closure with an exclusive `flock` on this file.
    ///
    /// If this file has already been locked elsewhere (via another file descriptor in this
    /// process, or in another process), this will block until it is able to acquire an
    /// exclusive lock.
    fn do_exclusive<F, T>(&self, action: F) -> Result<T>
    where
        F: FnOnce() -> T,
    {
        let guard = FileLockGuard(self);
        flock(self.as_raw_fd(), FlockArg::LockExclusive)?;
        let result = action();
        drop(guard); // Will happen implicitly anyway on exit from this function.
        Ok(result)
    }

    /// Call the given closure with a shared `flock` on this file.
    ///
    /// If this file has already been locked elsewhere (via another file descriptor in this
    /// process, or in another process), this will block until it is able to acquire a shared
    /// lock.
    fn do_shared<F, T>(&self, action: F) -> Result<T>
    where
        F: FnOnce() -> T,
    {
        let guard = FileLockGuard(self);
        flock(self.as_raw_fd(), FlockArg::LockShared)?;
        let result = action();
        drop(guard); // Will happen implicitly anyway on exit from this function.
        Ok(result)
    }
}

/// Guard used by `LockDo for File` to ensure that the given file is closed during unwinding, thus
/// releasing all locks.
struct FileLockGuard<'a>(&'a File);

impl<'a> Drop for FileLockGuard<'a> {
    fn drop(&mut self) {
        flock(self.0.as_raw_fd(), FlockArg::Unlock).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::LockDo;

    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;
    use std::path::Path;

    use nix::fcntl::{flock, FlockArg};

    fn can_lock<P: AsRef<Path>>(filename: P, exclusive: bool) -> bool {
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(filename)
            .unwrap();
        let mode = match exclusive {
            true => FlockArg::LockExclusiveNonblock,
            false => FlockArg::LockSharedNonblock,
        };
        match flock(file.as_raw_fd(), mode) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    fn can_lock_exclusive<P: AsRef<Path>>(filename: P) -> bool {
        can_lock(filename, true)
    }

    fn can_lock_shared<P: AsRef<Path>>(filename: P) -> bool {
        can_lock(filename, false)
    }

    #[test]
    fn file_do_exclusive_takes_exclusive_flock() {
        let lock_dir = tempdir::TempDir::new("locks").unwrap();
        let lock_filename = lock_dir.path().join("lock");
        let lock_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&lock_filename)
            .unwrap();

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));

        lock_file
            .do_exclusive(|| {
                assert!(!can_lock_exclusive(&lock_filename));
                assert!(!can_lock_shared(&lock_filename));
            })
            .unwrap();

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));
    }

    #[test]
    fn file_do_shared_takes_shared_flock() {
        let lock_dir = tempdir::TempDir::new("locks").unwrap();
        let lock_filename = lock_dir.path().join("lock");
        let lock_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&lock_filename)
            .unwrap();

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));

        lock_file
            .do_shared(|| {
                assert!(!can_lock_exclusive(&lock_filename));
                assert!(can_lock_shared(&lock_filename));
            })
            .unwrap();

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));
    }
}
