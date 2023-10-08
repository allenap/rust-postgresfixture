use std::env;
use std::ffi::OsString;
use std::path::Path;

type PrependedPath = Result<OsString, env::JoinPathsError>;

/// Prepend the given `dir` to the given `path`.
///
/// If `dir` is already in `path` it is moved to first place. Note that this does
/// *not* update `PATH` in the environment.
pub fn prepend_to_path(dir: &Path, path: Option<OsString>) -> PrependedPath {
    Ok(match path {
        None => env::join_paths([dir])?,
        Some(path) => {
            let mut paths = vec![dir.to_path_buf()];
            paths.extend(env::split_paths(&path).filter(|path| path != dir));
            env::join_paths(paths)?
        }
    })
}

#[cfg(test)]
mod tests {
    use std::env;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_prepend_to_path_prepends_given_dir_to_path() -> TestResult {
        let path = env::join_paths(&[
            tempdir::TempDir::new("aaa")?.path(),
            tempdir::TempDir::new("bbb")?.path(),
        ])?;
        let tempdir = tempdir::TempDir::new("bin")?;
        let expected = {
            let mut tmp = vec![tempdir.path().to_path_buf()];
            tmp.extend(env::split_paths(&path));
            env::join_paths(tmp)?
        };
        let observed = { super::prepend_to_path(tempdir.path(), Some(path))? };
        assert_eq!(expected, observed);
        Ok(())
    }

    #[test]
    fn test_prepend_to_path_moves_dir_to_front_of_path() -> TestResult {
        let tempdir = tempdir::TempDir::new("bin")?;
        let path = env::join_paths(&[
            tempdir::TempDir::new("aaa")?.path(),
            tempdir::TempDir::new("bbb")?.path(),
            tempdir.path(),
        ])?;
        let expected = {
            let mut tmp = vec![tempdir.path().to_path_buf()];
            tmp.extend(env::split_paths(&path).take(2));
            env::join_paths(tmp)?
        };
        let observed = { super::prepend_to_path(tempdir.path(), Some(path))? };
        assert_eq!(expected, observed);
        Ok(())
    }

    #[test]
    fn test_prepend_to_path_returns_given_dir_if_path_is_empty() -> TestResult {
        let tempdir = tempdir::TempDir::new("bin")?;
        let expected = tempdir.path();
        let observed = super::prepend_to_path(tempdir.path(), None)?;
        assert_eq!(expected, observed);
        Ok(())
    }
}
