//! File-based locking using [`flock(2)`](https://linux.die.net/man/2/flock).
//!
//! You must start with an [`UnlockedFile`].
//!
//! ```rust
//! let lock_dir = tempdir::TempDir::new("locks").unwrap();
//! # use postgresfixture::lock::UnlockedFile;
//! let mut lock = UnlockedFile::try_from(lock_dir.path().join("foo").as_path()).unwrap();
//! let lock = lock.lock_shared().unwrap();
//! let lock = lock.lock_exclusive().unwrap();
//! let lock = lock.unlock().unwrap();
//! ```
//!
//! Dropping a [`LockedFileShared`] or [`LockedFileExclusive`] drops their locks
//! too, by virtue of dropping the [`File`] they each wrap, so there's no need
//! to call `unlock` unless you prefer to be explicit.

use std::fs::File;
use std::os::unix::io::AsRawFd;

use either::{Either, Left, Right};
use nix::errno::Errno;
use nix::fcntl::{flock, FlockArg};
use nix::Result;
use uuid::Uuid;

#[derive(Debug)]
pub struct UnlockedFile(File);
#[derive(Debug)]
pub struct LockedFileShared(File);
#[derive(Debug)]
pub struct LockedFileExclusive(File);

impl From<File> for UnlockedFile {
    fn from(file: File) -> Self {
        Self(file)
    }
}

impl TryFrom<&std::path::Path> for UnlockedFile {
    type Error = std::io::Error;

    fn try_from(path: &std::path::Path) -> std::io::Result<Self> {
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .map(UnlockedFile)
    }
}

impl TryFrom<&Uuid> for UnlockedFile {
    type Error = std::io::Error;

    fn try_from(uuid: &Uuid) -> std::io::Result<Self> {
        let mut buffer = Uuid::encode_buffer();
        let uuid = uuid.simple().encode_lower(&mut buffer);
        let filename = ".postgresfixture.".to_owned() + uuid;
        let path = std::env::temp_dir().join(filename);
        UnlockedFile::try_from(&*path)
    }
}

#[allow(unused)]
impl UnlockedFile {
    pub fn try_lock_shared(self) -> Result<Either<Self, LockedFileShared>> {
        match flock(self.0.as_raw_fd(), FlockArg::LockSharedNonblock) {
            Ok(_) => Ok(Right(LockedFileShared(self.0))),
            Err(Errno::EAGAIN) => Ok(Left(self)),
            Err(err) => Err(err),
        }
    }

    pub fn lock_shared(self) -> Result<LockedFileShared> {
        flock(self.0.as_raw_fd(), FlockArg::LockShared)?;
        Ok(LockedFileShared(self.0))
    }

    pub fn try_lock_exclusive(self) -> Result<Either<Self, LockedFileExclusive>> {
        match flock(self.0.as_raw_fd(), FlockArg::LockExclusiveNonblock) {
            Ok(_) => Ok(Right(LockedFileExclusive(self.0))),
            Err(Errno::EAGAIN) => Ok(Left(self)),
            Err(err) => Err(err),
        }
    }

    pub fn lock_exclusive(self) -> Result<LockedFileExclusive> {
        flock(self.0.as_raw_fd(), FlockArg::LockExclusive)?;
        Ok(LockedFileExclusive(self.0))
    }
}

#[allow(unused)]
impl LockedFileShared {
    pub fn try_lock_exclusive(self) -> Result<Either<Self, LockedFileExclusive>> {
        match flock(self.0.as_raw_fd(), FlockArg::LockExclusiveNonblock) {
            Ok(_) => Ok(Right(LockedFileExclusive(self.0))),
            Err(Errno::EAGAIN) => Ok(Left(self)),
            Err(err) => Err(err),
        }
    }

    pub fn lock_exclusive(self) -> Result<LockedFileExclusive> {
        flock(self.0.as_raw_fd(), FlockArg::LockExclusive)?;
        Ok(LockedFileExclusive(self.0))
    }

    pub fn try_unlock(self) -> Result<Either<Self, UnlockedFile>> {
        match flock(self.0.as_raw_fd(), FlockArg::UnlockNonblock) {
            Ok(_) => Ok(Right(UnlockedFile(self.0))),
            Err(Errno::EAGAIN) => Ok(Left(self)),
            Err(err) => Err(err),
        }
    }

    pub fn unlock(self) -> Result<UnlockedFile> {
        flock(self.0.as_raw_fd(), FlockArg::Unlock)?;
        Ok(UnlockedFile(self.0))
    }
}

#[allow(unused)]
impl LockedFileExclusive {
    pub fn try_lock_shared(self) -> Result<Either<Self, LockedFileShared>> {
        match flock(self.0.as_raw_fd(), FlockArg::LockSharedNonblock) {
            Ok(_) => Ok(Right(LockedFileShared(self.0))),
            Err(Errno::EAGAIN) => Ok(Left(self)),
            Err(err) => Err(err),
        }
    }

    pub fn lock_shared(self) -> Result<LockedFileShared> {
        flock(self.0.as_raw_fd(), FlockArg::LockShared)?;
        Ok(LockedFileShared(self.0))
    }

    pub fn try_unlock(self) -> Result<Either<Self, UnlockedFile>> {
        match flock(self.0.as_raw_fd(), FlockArg::UnlockNonblock) {
            Ok(_) => Ok(Right(UnlockedFile(self.0))),
            Err(Errno::EAGAIN) => Ok(Left(self)),
            Err(err) => Err(err),
        }
    }

    pub fn unlock(self) -> Result<UnlockedFile> {
        flock(self.0.as_raw_fd(), FlockArg::Unlock)?;
        Ok(UnlockedFile(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::UnlockedFile;

    use std::fs::OpenOptions;
    use std::io;
    use std::os::unix::io::AsRawFd;
    use std::path::Path;

    use either::Left;
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
    fn file_lock_exclusive_takes_exclusive_flock() -> io::Result<()> {
        let lock_dir = tempdir::TempDir::new("locks").unwrap();
        let lock_filename = lock_dir.path().join("lock");
        let lock = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&lock_filename)
            .map(UnlockedFile::from)?;

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));

        let lock = lock.lock_exclusive()?;

        assert!(!can_lock_exclusive(&lock_filename));
        assert!(!can_lock_shared(&lock_filename));

        lock.unlock()?;

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));

        Ok(())
    }

    #[test]
    fn file_try_lock_exclusive_does_not_block_on_existing_shared_lock() -> io::Result<()> {
        let lock_dir = tempdir::TempDir::new("locks")?;
        let lock_filename = lock_dir.path().join("lock");
        let open_lock_file = || {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(&lock_filename)
                .map(UnlockedFile::from)
        };

        let _lock_shared = open_lock_file()?.lock_shared()?;

        assert!(match open_lock_file()?.try_lock_exclusive() {
            Ok(Left(_)) => true,
            _ => false,
        });

        Ok(())
    }

    #[test]
    fn file_try_lock_exclusive_does_not_block_on_existing_exclusive_lock() -> io::Result<()> {
        let lock_dir = tempdir::TempDir::new("locks")?;
        let lock_filename = lock_dir.path().join("lock");
        let open_lock_file = || {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(&lock_filename)
                .map(UnlockedFile::from)
        };

        let _lock_exclusive = open_lock_file()?.lock_exclusive()?;

        assert!(match open_lock_file()?.try_lock_exclusive() {
            Ok(Left(_)) => true,
            _ => false,
        });

        Ok(())
    }
}
