use std::env;
use std::ffi;
use std::path::Path;


/// Prepend the given path to the `PATH` environment variable, and return it.
///
/// If it's already in `PATH` it is moved to first place. Note that this does
/// *not* update `PATH` in the environment.
pub fn prepend_path(bindir: &Path)
        -> Result<Option<ffi::OsString>, env::JoinPathsError> {
    Ok(match env::var_os("PATH") {
        None => None,
        Some(path) => {
            let mut paths = vec!(bindir.to_path_buf());
            paths.extend(
                env::split_paths(&path)
                    .filter(|path| path != bindir));
            Some(env::join_paths(paths)?)
        },
    })
}


#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::prepend_path;

    use std::env;

    #[test]
    fn test_prepend_path() {
        let path = env::var_os("PATH").unwrap();
        let tmpdir = tempdir::TempDir::new("bin").unwrap();
        // let bindir = PathBuf::from("foo/bar");
        let expected = {
            let mut tmp = vec!(tmpdir.path().to_path_buf());
            tmp.extend(env::split_paths(&path));
            env::join_paths(tmp).unwrap()
        };
        let observed = {
            prepend_path(tmpdir.path()).unwrap().unwrap()
        };
        assert_eq!(expected, observed);
    }

}
