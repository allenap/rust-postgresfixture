//! Version cache for binaries.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs::File;
use std::hash::Hasher;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::RwLock;

use super::RuntimeError;
use crate::version::{Version, VersionError};

#[derive(Debug)]
struct Entry {
    size: u64,
    hash: u64,
    version: Version,
}

lazy_static! {
    static ref CACHE: RwLock<HashMap<PathBuf, Entry>> = HashMap::new().into();
}

/// Get a cached version of PostgreSQL from a given PostgreSQL binary.
///
/// If the binary referenced has changed, as measured by size and a hash
/// calculated from its contents, this will run the binary again to determine
/// the version. Even with hashing, a cache hit turns out to be ~10x faster than
/// running `pg_ctl -version` (and adds 200-300µs to a cache miss).
///
/// The [PostgreSQL "Versioning Policy"][versioning] shows that version numbers
/// are **not** SemVer compatible. The [`version`][`mod@crate::version`] module
/// in this crate is used to parse the version string from `pg_ctl` and it does
/// understand the nuances of PostgreSQL's versioning scheme.
///
/// [versioning]: https://www.postgresql.org/support/versioning/
pub fn version<P: AsRef<Path>>(binary: P) -> Result<Version, RuntimeError> {
    let binary: PathBuf = binary.as_ref().canonicalize()?;
    let (size, hash) = {
        let mut file = File::open(&binary)?;
        let size = file.metadata()?.len();
        let hash = {
            let mut hasher = DefaultHasher::new();
            let mut buffer = [0u8; 16384]; // 16 kiB buffer.
            loop {
                let bytes_read = file.read(&mut buffer)?;
                if bytes_read == 0 {
                    break; // Reached end of file
                }
                hasher.write(&buffer[..bytes_read]);
            }
            hasher.finish()
        };
        (size, hash)
    };

    // Try to check if we already know the version.
    if let Ok(cache) = CACHE.read() {
        if let Some(entry) = cache.get(&binary) {
            if entry.size == size && entry.hash == hash {
                return Ok(entry.version);
            }
        }
    }

    // Okay, we definitely need to check the version.
    let version = version_from_binary(&binary)?;

    // Try to cache the version.
    if let Ok(mut cache) = CACHE.write() {
        cache.insert(binary, Entry { size, hash, version });
    }

    Ok(version)
}

/// Get the version of PostgreSQL from a given PostgreSQL binary.
///
/// The [PostgreSQL "Versioning Policy"][versioning] shows that version numbers
/// are **not** SemVer compatible. The [`version`][`mod@crate::version`] module
/// in this crate is used to parse the version string from `pg_ctl` and it does
/// understand the nuances of PostgreSQL's versioning scheme.
///
/// [versioning]: https://www.postgresql.org/support/versioning/
fn version_from_binary<P: AsRef<Path>>(binary: P) -> Result<Version, RuntimeError> {
    let output = Command::new(binary.as_ref()).arg("--version").output()?;
    if output.status.success() {
        let version_string = String::from_utf8_lossy(&output.stdout);
        // The version parser can deal with leading garbage, i.e. it can parse
        // "pg_ctl (PostgreSQL) 12.2" and get 12.2 out of it.
        Ok(version_string.parse()?)
    } else {
        Err(VersionError::Missing)?
    }
}
