use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::version;

use super::Runtime;

pub type Runtimes<'a> = Box<dyn Iterator<Item = Runtime> + 'a>;

pub trait RuntimeStrategy {
    /// Find all runtimes that this strategy knows about.
    fn runtimes(&self) -> Runtimes;

    /// Select the most appropriate runtime known to this strategy for the given
    /// version constraint.
    fn select(&self, version: &version::PartialVersion) -> Option<Runtime> {
        self.runtimes()
            .filter(|runtime| version.compatible(runtime.version))
            .max_by(|ra, rb| ra.version.cmp(&rb.version))
    }

    /// The runtime to use when there are no version constraints, e.g. when
    /// creating a new cluster.
    fn fallback(&self) -> Option<Runtime> {
        self.runtimes().max_by(|ra, rb| ra.version.cmp(&rb.version))
    }
}

/// Find runtimes on the given path, or on `PATH` (from the environment).
///
/// Parses input according to platform conventions for the `PATH` environment
/// variable. See [`env::split_paths`] for details.
#[derive(Clone, Debug)]
pub enum RuntimesOnPath {
    /// Find runtimes on the given path.
    Custom(PathBuf),
    /// Find runtimes on `PATH` (environment variable).
    Env,
}

impl RuntimesOnPath {
    fn find_on_path<T: AsRef<OsStr> + ?Sized>(path: &T) -> Vec<PathBuf> {
        env::split_paths(path)
            .filter(|bindir| bindir.join("pg_ctl").exists())
            .collect()
    }

    fn find_on_env_path() -> Vec<PathBuf> {
        match env::var_os("PATH") {
            Some(path) => Self::find_on_path(&path),
            None => vec![],
        }
    }
}

impl RuntimeStrategy for RuntimesOnPath {
    fn runtimes(&self) -> Runtimes {
        Box::new(
            match self {
                RuntimesOnPath::Custom(path) => Self::find_on_path(path),
                RuntimesOnPath::Env => Self::find_on_env_path(),
            }
            .into_iter()
            // Throw away runtimes that we can't determine the version for.
            .filter_map(|bindir| Runtime::new(bindir).ok()),
        )
    }
}

#[derive(Clone, Debug)]
pub struct RuntimesOnPlatform;

impl RuntimesOnPlatform {
    /// Find runtimes using platform-specific knowledge (Linux).
    ///
    /// For example: on Debian and Ubuntu, check `/usr/lib/postgresql`.
    #[cfg(any(doc, target_os = "linux"))]
    pub fn find() -> Vec<PathBuf> {
        glob::glob("/usr/lib/postgresql/*/bin/pg_ctl")
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter(|path| path.is_file())
                    .filter_map(|path| path.parent().map(Path::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find runtimes using platform-specific knowledge (macOS).
    ///
    /// For example: check Homebrew.
    #[cfg(any(doc, target_os = "macos"))]
    pub fn find() -> Vec<PathBuf> {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        std::process::Command::new("brew")
            .arg("--prefix")
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    Some(OsString::from_vec(output.stdout))
                } else {
                    None
                }
            })
            .and_then(|brew_prefix| {
                glob::glob(&format!(
                    "{}/Cellar/postgresql@*/*/bin/pg_ctl",
                    brew_prefix.to_string_lossy().trim_end()
                ))
                .ok()
            })
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter(|path| path.is_file())
                    .filter_map(|path| path.parent().map(Path::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl RuntimeStrategy for RuntimesOnPlatform {
    fn runtimes(&self) -> Runtimes {
        Box::new(
            Self::find()
                .into_iter()
                // Throw away runtimes that we can't determine the version for.
                .filter_map(|bindir| Runtime::new(bindir).ok()),
        )
    }
}

pub struct RuntimeStrategySet(Vec<Box<dyn RuntimeStrategy>>);

impl RuntimeStrategy for RuntimeStrategySet {
    fn runtimes(&self) -> Runtimes {
        Box::new(self.0.iter().flat_map(|strategy| strategy.runtimes()))
    }

    fn select(&self, version: &version::PartialVersion) -> Option<Runtime> {
        self.0
            .iter()
            .filter_map(|strategy| strategy.select(version))
            .next()
    }

    fn fallback(&self) -> Option<Runtime> {
        self.0
            .iter()
            .filter_map(|strategy| strategy.fallback())
            .next()
    }
}

impl Default for RuntimeStrategySet {
    fn default() -> Self {
        Self(vec![
            Box::new(RuntimesOnPath::Env),
            Box::new(RuntimesOnPlatform),
        ])
    }
}

impl RuntimeStrategy for Runtime {
    fn runtimes(&self) -> Runtimes {
        Box::new(std::iter::once(self.clone()))
    }

    fn select(&self, version: &version::PartialVersion) -> Option<Runtime> {
        if version.compatible(self.version) {
            Some(self.clone())
        } else {
            None
        }
    }

    fn fallback(&self) -> Option<Runtime> {
        Some(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::{RuntimeStrategy, RuntimesOnPath, RuntimesOnPlatform};

    #[test]
    fn runtime_find_custom_path() {
        let path = env::var_os("PATH").expect("PATH not set");
        let strategy = RuntimesOnPath::Custom(path.into());
        let runtimes = strategy.runtimes();
        assert_ne!(0, runtimes.count());
    }

    #[test]
    fn runtime_find_env_path() {
        let runtimes = RuntimesOnPath::Env.runtimes();
        assert_ne!(0, runtimes.count());
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn runtime_find_on_platform() {
        let runtimes = RuntimesOnPlatform.runtimes();
        assert_ne!(0, runtimes.count());
    }

    // TODO: Test `RuntimeStrategySet`.
}
