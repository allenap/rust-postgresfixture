use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::version;

use super::{Runtime, Runtimes, Strategy};

/// Find runtimes on a given path, or on `PATH` (from the environment).
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

impl Strategy for RuntimesOnPath {
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

/// Find runtimes using platform-specific knowledge.
///
/// For example:
/// - on Debian and Ubuntu, check subdirectories of `/usr/lib/postgresql`.
/// - on macOS, check Homebrew.
///
/// More platform-specific knowledge may be added to this strategy in the
/// future.
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
                    .filter_map(Result::ok)
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
                    .filter_map(Result::ok)
                    .filter(|path| path.is_file())
                    .filter_map(|path| path.parent().map(Path::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Strategy for RuntimesOnPlatform {
    fn runtimes(&self) -> Runtimes {
        Box::new(
            Self::find()
                .into_iter()
                // Throw away runtimes that we can't determine the version for.
                .filter_map(|bindir| Runtime::new(bindir).ok()),
        )
    }
}

/// Combine multiple runtime strategies, in order of preference.
pub struct StrategySet(Vec<Box<dyn Strategy>>);

impl Strategy for StrategySet {
    /// Runtimes known to all strategies, in the same order as each strategy
    /// returns them.
    ///
    /// Note that runtimes are deduplicated by version number, i.e. if a runtime
    /// with the same version number appears in multiple strategies, it will
    /// only be returned the first time it is seen.
    fn runtimes(&self) -> Runtimes {
        let mut seen = std::collections::HashSet::new();
        Box::new(
            self.0
                .iter()
                .flat_map(|strategy| strategy.runtimes())
                .filter(move |runtime| seen.insert(runtime.version)),
        )
    }

    /// Asks each strategy in turn to select a runtime. The first non-[`None`]
    /// answer is selected.
    fn select(&self, version: &version::PartialVersion) -> Option<Runtime> {
        self.0.iter().find_map(|strategy| strategy.select(version))
    }

    /// Asks each strategy in turn for a fallback runtime. The first
    /// non-[`None`] answer is selected.
    fn fallback(&self) -> Option<Runtime> {
        self.0.iter().find_map(|strategy| strategy.fallback())
    }
}

/// Select runtimes from on `PATH` followed by platform-specific runtimes.
impl Default for StrategySet {
    fn default() -> Self {
        Self(vec![
            Box::new(RuntimesOnPath::Env),
            Box::new(RuntimesOnPlatform),
        ])
    }
}

/// Use a single runtime as a strategy.
impl Strategy for Runtime {
    /// This runtime itself is the only runtime known to this strategy.
    fn runtimes(&self) -> Runtimes {
        Box::new(std::iter::once(self.clone()))
    }

    /// Return this runtime if the given version constraint is compatible.
    fn select(&self, version: &version::PartialVersion) -> Option<Runtime> {
        if version.compatible(self.version) {
            Some(self.clone())
        } else {
            None
        }
    }

    /// Always return this runtime.
    fn fallback(&self) -> Option<Runtime> {
        Some(self.clone())
    }
}

/// The default runtime strategy.
///
/// At present this returns the default [`StrategySet`].
pub fn default() -> impl Strategy {
    StrategySet::default()
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::{RuntimesOnPath, RuntimesOnPlatform, Strategy, StrategySet};

    /// This will fail if there are no PostgreSQL runtimes installed.
    #[test]
    fn runtime_find_custom_path() {
        let path = env::var_os("PATH").expect("PATH not set");
        let strategy = RuntimesOnPath::Custom(path.into());
        let runtimes = strategy.runtimes();
        assert_ne!(0, runtimes.count());
    }

    /// This will fail if there are no PostgreSQL runtimes installed.
    #[test]
    fn runtime_find_env_path() {
        let runtimes = RuntimesOnPath::Env.runtimes();
        assert_ne!(0, runtimes.count());
    }

    /// This will fail if there are no PostgreSQL runtimes installed.
    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn runtime_find_on_platform() {
        let runtimes = RuntimesOnPlatform.runtimes();
        assert_ne!(0, runtimes.count());
    }

    /// This will fail if there are no PostgreSQL runtimes installed. It's also
    /// somewhat fragile because it relies upon knowing the implementation of
    /// the strategies of which the default [`StrategySet`] is composed.
    #[test]
    fn runtime_strategy_set_default() {
        let strategy = StrategySet::default();
        // There is at least one runtime available.
        let runtimes = strategy.runtimes();
        assert_ne!(0, runtimes.count());
        // There is always a fallback.
        assert!(strategy.fallback().is_some());
    }
}
