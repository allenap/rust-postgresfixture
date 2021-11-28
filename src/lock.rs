use std::fs::File;
use std::os::unix::io::AsRawFd;

use nix::fcntl::{flock, FlockArg};
use nix::Result;

pub struct FileLock(File);

impl FileLock {
    /// Create a new `FileLock` for the given `File`.
    pub fn from(file: File) -> Self {
        Self(file)
    }

    /// Call the given closure with an exclusive `flock` on this file.
    ///
    /// If this file has already been locked elsewhere (via another file
    /// descriptor in this process, or in another process), this will NOT block
    /// until it is able to acquire an exclusive lock, and instead return
    /// `Err(nix::errno::Errno::EAGAIN)`.
    pub fn do_exclusive<F, T>(&mut self, action: F) -> Result<T>
    where
        F: FnOnce() -> T,
    {
        let guard = FileLockGuard(&self.0, FlockArg::Unlock);
        flock(self.0.as_raw_fd(), FlockArg::LockExclusiveNonblock)?;
        let result = action();
        drop(guard);
        Ok(result)
    }

    /// Call the given closure with a shared `flock` on this file.
    ///
    /// If this file has already been locked elsewhere (via another file
    /// descriptor in this process, or in another process), this will block
    /// until it is able to acquire a shared lock.
    pub fn do_shared<F, T>(&mut self, action: F) -> Result<T>
    where
        F: FnOnce() -> T,
    {
        let guard = FileLockGuard(&self.0, FlockArg::Unlock);
        flock(self.0.as_raw_fd(), FlockArg::LockShared)?;
        let result = action();
        drop(guard);
        Ok(result)
    }
}

/// Guard used to ensure that locks are downgraded or released during unwinding.
struct FileLockGuard<'a>(&'a File, FlockArg);

impl<'a> Drop for FileLockGuard<'a> {
    fn drop(&mut self) {
        flock(self.0.as_raw_fd(), self.1).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::FileLock;

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
        flock(file.as_raw_fd(), mode).is_ok()
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
        let mut lock = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&lock_filename)
            .map(FileLock::from)
            .unwrap();

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));

        lock.do_exclusive(|| {
            assert!(!can_lock_exclusive(&lock_filename));
            assert!(!can_lock_shared(&lock_filename));
        })
        .unwrap();

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));
    }

    #[test]
    fn file_do_exclusive_does_not_block_on_existing_shared_lock() {
        let lock_dir = tempdir::TempDir::new("locks").unwrap();
        let lock_filename = lock_dir.path().join("lock");
        let open_lock_file = || {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(&lock_filename)
                .map(FileLock::from)
                .unwrap()
        };

        assert!(open_lock_file()
            .do_shared(|| match open_lock_file().do_exclusive(|| false) {
                Err(nix::errno::Errno::EAGAIN) => true,
                other => other.unwrap(),
            })
            .unwrap());
    }

    #[test]
    fn file_do_exclusive_does_not_block_on_existing_exclusive_lock() {
        let lock_dir = tempdir::TempDir::new("locks").unwrap();
        let lock_filename = lock_dir.path().join("lock");
        let open_lock_file = || {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(&lock_filename)
                .map(FileLock::from)
                .unwrap()
        };

        assert!(open_lock_file()
            .do_exclusive(|| match open_lock_file().do_exclusive(|| false) {
                Err(nix::errno::Errno::EAGAIN) => true,
                other => other.unwrap(),
            })
            .unwrap());
    }

    #[test]
    fn file_do_shared_takes_shared_flock() {
        let lock_dir = tempdir::TempDir::new("locks").unwrap();
        let lock_filename = lock_dir.path().join("lock");
        let mut lock = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&lock_filename)
            .map(FileLock::from)
            .unwrap();

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));

        lock.do_shared(|| {
            assert!(!can_lock_exclusive(&lock_filename));
            assert!(can_lock_shared(&lock_filename));
        })
        .unwrap();

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));
    }
}
