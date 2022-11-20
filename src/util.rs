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

    #[test]
    fn test_prepend_to_path_prepends_given_dir_to_path() {
        let path = env::join_paths(&[
            tempdir::TempDir::new("aaa").unwrap().path(),
            tempdir::TempDir::new("bbb").unwrap().path(),
        ])
        .unwrap();
        let tmpdir = tempdir::TempDir::new("bin").unwrap();
        let expected = {
            let mut tmp = vec![tmpdir.path().to_path_buf()];
            tmp.extend(env::split_paths(&path));
            env::join_paths(tmp).unwrap()
        };
        let observed = { super::prepend_to_path(tmpdir.path(), Some(path)).unwrap() };
        assert_eq!(expected, observed);
    }

    #[test]
    fn test_prepend_to_path_moves_dir_to_front_of_path() {
        let tmpdir = tempdir::TempDir::new("bin").unwrap();
        let path = env::join_paths(&[
            tempdir::TempDir::new("aaa").unwrap().path(),
            tempdir::TempDir::new("bbb").unwrap().path(),
            tmpdir.path(),
        ])
        .unwrap();
        let expected = {
            let mut tmp = vec![tmpdir.path().to_path_buf()];
            tmp.extend(env::split_paths(&path).take(2));
            env::join_paths(tmp).unwrap()
        };
        let observed = { super::prepend_to_path(tmpdir.path(), Some(path)).unwrap() };
        assert_eq!(expected, observed);
    }

    #[test]
    fn test_prepend_to_path_returns_given_dir_if_path_is_empty() {
        let tmpdir = tempdir::TempDir::new("bin").unwrap();
        let expected = tmpdir.path();
        let observed = super::prepend_to_path(tmpdir.path(), None).unwrap();
        assert_eq!(expected, observed);
    }
}
