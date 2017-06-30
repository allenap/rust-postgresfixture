use std::fs::File;
use std::io;
use std::os::unix::io::AsRawFd;

use nix::fcntl::{flock, FlockArg};


// pub fn exclusive<P, F, T>(filename: P, action: F) -> io::Result<T>
//     where P: AsRef<Path>, F: FnOnce() -> T
// {
//     _flock(filename, FlockArg::LockExclusive, action)
// }
//
// pub fn shared<P, F, T>(filename: P, action: F) -> io::Result<T>
//     where P: AsRef<Path>, F: FnOnce() -> T
// {
//     _flock(filename, FlockArg::LockShared, action)
// }
//
// fn _flock<P, F, T>(filename: P, arg: FlockArg, action: F) -> io::Result<T>
//     where P: AsRef<Path>, F: FnOnce() -> T
// {
//     let file = OpenOptions::new()
//         .append(true).create(true).open(filename)?;
//     flock(file.as_raw_fd(), arg)?;
//     Ok(action())
// }


pub trait LockDo {

    fn do_exclusive<F, T>(&self, action: F) -> io::Result<T>
        where F: FnOnce() -> T;

    fn do_shared<F, T>(&self, action: F) -> io::Result<T>
        where F: FnOnce() -> T;

}


struct FileLockGuard<'a>(&'a File);

impl<'a> Drop for FileLockGuard<'a> {
    fn drop(&mut self) {
        flock(self.0.as_raw_fd(), FlockArg::Unlock).unwrap()
    }
}


impl LockDo for File {

    fn do_exclusive<F, T>(&self, action: F) -> io::Result<T>
        where F: FnOnce() -> T
    {
        let guard = FileLockGuard(&self);
        flock(self.as_raw_fd(), FlockArg::LockExclusive)?;
        let result = action();
        drop(guard);  // Will happen implicitly anyway.
        Ok(result)
    }

    fn do_shared<F, T>(&self, action: F) -> io::Result<T>
        where F: FnOnce() -> T
    {
        let guard = FileLockGuard(&self);
        flock(self.as_raw_fd(), FlockArg::LockShared)?;
        let result = action();
        drop(guard);  // Will happen implicitly anyway.
        Ok(result)
    }

}


#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::LockDo;

    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;
    use std::path::Path;

    use nix::fcntl::{flock, FlockArg};

    fn can_lock<P: AsRef<Path>>(filename: P, exclusive: bool) -> bool {
        let file = OpenOptions::new().append(true).create(true).open(filename).unwrap();
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
        let lock_file = OpenOptions::new().append(true).create(true).open(&lock_filename).unwrap();

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));

        lock_file.do_exclusive(|| {
            assert!(!can_lock_exclusive(&lock_filename));
            assert!(!can_lock_shared(&lock_filename));
        }).unwrap();

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));
    }

    #[test]
    fn file_do_shared_takes_shared_flock() {
        let lock_dir = tempdir::TempDir::new("locks").unwrap();
        let lock_filename = lock_dir.path().join("lock");
        let lock_file = OpenOptions::new().append(true).create(true).open(&lock_filename).unwrap();

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));

        lock_file.do_shared(|| {
            assert!(!can_lock_exclusive(&lock_filename));
            assert!(can_lock_shared(&lock_filename));
        }).unwrap();

        assert!(can_lock_exclusive(&lock_filename));
        assert!(can_lock_shared(&lock_filename));
    }
}
