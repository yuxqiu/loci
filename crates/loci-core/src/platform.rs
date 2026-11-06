//! What differs between Unix and Windows: each difference is a function here (a `cfg`
//! pair, or `cfg` lines inside one), so the rest of loci has no `cfg` (research 24).
//! The unit tests of what only Unix has are here too.

use std::fs::Metadata;
use std::process::ExitStatus;

/// Gives `builder`'s files the mode a plain write gives (0o666 less the umask), not
/// tempfile's owner-only 0o600.
#[cfg(unix)]
pub(crate) fn plain_file_mode(builder: &mut tempfile::Builder) {
    builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o666));
}

/// Windows has no mode bits.
#[cfg(not(unix))]
pub(crate) fn plain_file_mode(_: &mut tempfile::Builder) {}

/// Whether the file can be run: one of its execute bits is set.
#[cfg(unix)]
pub(crate) fn can_execute(meta: &Metadata) -> bool {
    std::os::unix::fs::PermissionsExt::mode(&meta.permissions()) & 0o111 != 0
}

/// Windows has no execute bit: any file can be run (plugin discovery has required the
/// `.exe` name).
#[cfg(not(unix))]
pub(crate) fn can_execute(_: &Metadata) -> bool {
    true
}

/// The signal that ended a process that has no exit code.
#[cfg(unix)]
pub(crate) fn signal(status: &ExitStatus) -> Option<i32> {
    std::os::unix::process::ExitStatusExt::signal(status)
}

/// A Windows process always has an exit code.
#[cfg(not(unix))]
pub(crate) fn signal(_: &ExitStatus) -> Option<i32> {
    None
}

/// Makes a test's file runnable.
#[cfg(all(test, unix))]
pub(crate) fn make_executable(path: &std::path::Path) {
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
}

#[cfg(all(test, not(unix)))]
pub(crate) fn make_executable(_: &std::path::Path) {}

/// A test's link `link` to the folder `target`. On Windows, without Developer Mode,
/// creating a link needs administrator rights.
#[cfg(test)]
pub(crate) fn symlink_dir(target: &std::path::Path, link: &std::path::Path) {
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(target, link);
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_dir(target, link);
    made.unwrap_or_else(|e| panic!("cannot link {}: {e} (on Windows, enable Developer Mode)", link.display()));
}

/// Unit tests of what only Unix has: file modes, names Windows forbids and, except on
/// macOS, names that are bytes.
#[cfg(all(test, unix))]
mod unix_tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use crate::git::{Changes, Repo, tests::test_repo};
    use crate::vault::write_atomic;

    fn mode(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_rewritten_file_keeps_its_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.md");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o664)).unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!((std::fs::read_to_string(&path).unwrap(), mode(&path)), ("new".into(), 0o664));
    }

    #[test]
    fn a_new_file_gets_the_mode_of_a_plain_write() {
        let dir = tempfile::tempdir().unwrap();
        let (plain, staged) = (dir.path().join("plain"), dir.path().join("staged"));
        std::fs::write(&plain, "x").unwrap();
        write_atomic(&staged, b"x").unwrap();
        assert_eq!(mode(&staged), mode(&plain));
    }

    /// A permission-only change is not a change to the content.
    #[test]
    fn status_ignores_file_modes() {
        let tmp = test_repo();
        let root = tmp.path();
        std::fs::write(root.join("a.md"), "a\n").unwrap();
        Repo::new(root).run(&["add", "."]).unwrap();
        Repo::new(root).run(&["commit", "-qm", "base"]).unwrap();
        std::fs::set_permissions(root.join("a.md"), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(Repo::new(root).status().unwrap(), Changes::default());
    }

    /// A changed name that is no vault path (a control character) is an error, not a
    /// panic: a file whose deletion is staged is no longer listed, so opening the vault
    /// does not refuse it.
    #[test]
    fn status_refuses_a_name_that_is_no_vault_path() {
        let tmp = test_repo();
        let root = tmp.path();
        let git = |args: &[&str]| Repo::new(root).run(args).unwrap();
        std::fs::write(root.join("a\nb.md"), "a\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "base"]);
        std::fs::remove_file(root.join("a\nb.md")).unwrap();
        let err = Repo::new(root).status().unwrap_err();
        assert!(err.contains(r#""a\nb.md" is not a vault path"#), "{err}");
    }

    /// macOS's file system refuses names that are bytes.
    #[cfg(not(target_os = "macos"))]
    mod byte_names {
        use std::os::unix::ffi::OsStrExt;

        use crate::git::{Repo, tests::test_repo};

        /// A changed name that is not UTF-8 is an error too, not a change left uncounted.
        #[test]
        fn status_refuses_a_name_that_is_not_utf8() {
            let tmp = test_repo();
            let root = tmp.path();
            let name = std::ffi::OsStr::from_bytes(b"n\xe9.md");
            std::fs::write(root.join(name), "n\n").unwrap();
            Repo::new(root).run(&["add", "."]).unwrap();
            Repo::new(root).run(&["commit", "-qm", "base"]).unwrap();
            std::fs::remove_file(root.join(name)).unwrap();
            let err = Repo::new(root).status().unwrap_err();
            assert!(err.contains(r#""n\xe9.md" has a name that is not UTF-8"#), "{err}");
        }
    }
}
