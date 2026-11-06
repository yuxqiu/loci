//! What differs between Unix and Windows for the tests: each difference is a function
//! here (a `cfg` pair, or `cfg` lines inside one), so the test files have no `cfg`
//! (research 24).

use std::path::Path;

/// Links `link` to the file `target`.
pub fn symlink_file(target: &Path, link: &Path) {
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(target, link);
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(target, link);
    made.unwrap_or_else(|e| link_failed(link, e));
}

/// Links `link` to the folder `target`.
pub fn symlink_dir(target: &Path, link: &Path) {
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(target, link);
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_dir(target, link);
    made.unwrap_or_else(|e| link_failed(link, e));
}

/// Removes the link `link` to a folder: Windows removes it as a folder, Unix as a file.
#[cfg(unix)]
pub fn remove_symlink_dir(link: &Path) {
    std::fs::remove_file(link).unwrap();
}

#[cfg(windows)]
pub fn remove_symlink_dir(link: &Path) {
    std::fs::remove_dir(link).unwrap();
}

/// Windows lets only an administrator make a link, unless Developer Mode is on (CI's
/// runners have it on).
fn link_failed(link: &Path, e: std::io::Error) -> ! {
    panic!("cannot link {}: {e} (on Windows, enable Developer Mode)", link.display())
}

/// Makes the file runnable.
#[cfg(unix)]
pub fn make_executable(path: &Path) {
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
}

/// Windows has no execute bit: any file can be run.
#[cfg(windows)]
pub fn make_executable(_: &Path) {}

/// Makes the host's program `name` runnable from the test `PATH` folder `bin`: links it
/// there.
#[cfg(unix)]
pub fn link_tool(bin: &Path, name: &str) {
    let path = std::env::var_os("PATH").expect("PATH is set");
    let found = std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("`{name}` is not on PATH"));
    symlink_file(&found, &bin.join(name));
}

/// Makes `git`, `sh`, `grep` or `head` runnable from the test `PATH` folder `bin`: links
/// Git for Windows' wrapper for `git` or `sh` there. The wrappers find their install
/// through their own links, where a link to the real program may miss its DLLs; the
/// `sh` wrapper puts Git's tools (`grep`, `head`) on its `PATH`, so those need no link.
#[cfg(windows)]
pub fn link_tool(bin: &Path, name: &str) {
    // `git --exec-path` is `<root>/<mingw64 or clangarm64>/libexec/git-core`.
    let out = std::process::Command::new("git").arg("--exec-path").output().unwrap();
    let exec_path = std::path::PathBuf::from(String::from_utf8(out.stdout).unwrap().trim());
    let root = exec_path.ancestors().nth(3).expect("git's exec path is three folders into its install");
    let wrapper = match name {
        "git" => root.join("cmd/git.exe"),
        "sh" => root.join("bin/sh.exe"),
        "grep" | "head" => return,
        other => panic!("no Git for Windows wrapper for `{other}`"),
    };
    symlink_file(&wrapper, &bin.join(format!("{name}.exe")));
}
