//! What only Unix has: file modes, names that are bytes, sockets (research 24). Every
//! other test runs on Windows too.
#![cfg(unix)]

mod common;

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;

use common::*;

/// Every name git reports must be UTF-8, even where core does not read (`plugins/`) and
/// in HEAD alone (a staged deletion): a vault holding one that is not is refused
/// (`file.unreadable`), naming it.
#[test]
fn a_non_utf8_name_anywhere_refuses_the_vault() {
    let v = TestVault::committed(BIN, "mv-rename");
    let raw = |p: &[u8]| v.root.join(std::ffi::OsStr::from_bytes(p));
    std::fs::create_dir_all(v.root.join("plugins/x")).unwrap();
    std::fs::write(raw(b"plugins/x/data\xff"), "x").unwrap();
    v.commit_all("odd");
    for args in [&["lint"][..], &["status"]] {
        let (code, env) = v.run(args);
        assert_eq!((code, error_code(&env)), (1, "file.unreadable"), "{args:?}: {env:#}");
        let message = error_message(&env);
        assert!(message.contains(r#""plugins/x/data\xff" has a name that is not UTF-8"#), "{message}");
    }
    std::fs::remove_file(raw(b"plugins/x/data\xff")).unwrap();
    let (code, env) = v.run(&["lint"]);
    assert_eq!((code, error_code(&env)), (1, "file.unreadable"), "a tracked name gone from disk: {env:#}");
    // A staged deletion leaves the name in HEAD alone: still refused, until committed.
    v.git(&["add", "-A"]);
    for args in [&["lint"][..], &["status"], &["lint", "--fix"]] {
        let (code, env) = v.run(args);
        assert_eq!((code, error_code(&env)), (1, "file.unreadable"), "{args:?}, a staged deletion: {env:#}");
    }
    v.commit_all("odd removed");
    // An ignored file is never named, so its name is not checked.
    v.write(".gitignore", "/ignored/\n");
    std::fs::create_dir_all(v.root.join("ignored")).unwrap();
    std::fs::write(raw(b"ignored/data\xff"), "x").unwrap();
    let (code, env) = v.run(&["lint"]);
    assert!(error_code(&env).is_empty() && code <= 1, "{env:#}");
}

/// A link with a name that is not UTF-8 is refused for its name.
#[test]
fn a_link_with_a_non_utf8_name_is_refused() {
    let v = TestVault::committed(BIN, "mv-rename");
    std::fs::write(v.tmp.path().join("secret.md"), "secret\n").unwrap();
    let odd = v.root.join(OsStr::from_bytes(b"views/odd\xff"));
    std::os::unix::fs::symlink(v.tmp.path().join("secret.md"), &odd).unwrap();
    let (code, env) = v.run(&["lint"]);
    assert_eq!((code, error_code(&env)), (1, "file.unreadable"), "{env:#}");
    assert!(error_message(&env).contains("has a name that is not UTF-8"), "{env:#}");
    std::fs::remove_file(&odd).unwrap();
    let (code, env) = v.run(&["lint"]);
    assert!(error_code(&env).is_empty() && code <= 1, "{env:#}");
}

/// Every file core reads must be readable: one that is not, by content, permission,
/// folder or name, refuses the vault for every command, naming each.
#[test]
fn an_unreadable_file_refuses_the_vault() {
    let v = TestVault::committed(BIN, "kinds");
    v.write("views/closed/inner.md", "# Inner\n");
    v.commit_all("inner");
    std::fs::write(v.root.join("views/bad.md"), b"# Bad \xff\n").unwrap();
    v.write("views/locked.md", "# Locked\n");
    std::fs::write(v.root.join(std::ffi::OsStr::from_bytes(b"views/odd\xff.png")), "x").unwrap();
    v.write("inbox/shut/x.md", "# X\n");
    let mode = |p: &str, m: u32| std::fs::set_permissions(v.root.join(p), std::fs::Permissions::from_mode(m)).unwrap();
    std::fs::create_dir_all(v.root.join("sources/papers")).unwrap();
    std::fs::write(v.root.join("sources/papers/p.pdf"), "%PDF").unwrap();
    for p in ["views/locked.md", "views/closed", "inbox/shut", "sources/papers/p.pdf"] {
        mode(p, 0o000);
    }
    let runs: Vec<_> =
        [&["lint"][..], &["status"], &["new", "concepts/a/fresh"]].iter().map(|args| v.run(args)).collect();
    for p in ["views/closed", "inbox/shut"] {
        mode(p, 0o755);
    }
    for (code, env) in runs {
        assert_eq!((code, error_code(&env)), (1, "file.unreadable"), "{env:#}");
        let message = error_message(&env);
        for part in [
            "`views/bad.md` is not valid UTF-8",
            "`views/locked.md` cannot be read",
            "`sources/papers/p.pdf` cannot be read",
            "`views/closed/inner.md` cannot be read",
            r#""views/odd\xff.png" has a name that is not UTF-8"#,
            "git: warning: could not open directory 'inbox/shut/'",
        ] {
            assert!(message.contains(part), "{part}: {message}");
        }
        // One problem per line, under the header.
        assert_eq!(message.lines().count(), 8, "{message}");
        assert!(message.lines().skip(1).all(|l| l.starts_with("  ")), "{message}");
    }
    assert!(!v.exists("concepts/a/fresh.md"), "nothing written");
}

/// A tracked file replaced by a socket or FIFO, which opening could block on, refuses the
/// vault.
#[test]
fn a_tracked_file_replaced_by_a_socket_refuses_the_vault() {
    let v = TestVault::with_files(BIN, &[("views/b.md", "# B\n")]);
    std::fs::remove_file(v.root.join("views/b.md")).unwrap();
    let _socket = UnixListener::bind(v.root.join("views/b.md")).unwrap();
    let (code, env) = v.run(&["lint"]);
    assert_eq!((code, error_code(&env)), (1, "file.unreadable"), "{env:#}");
    assert!(error_message(&env).contains("`views/b.md` is not a regular file"), "{env:#}");
}

/// git's fsmonitor daemon listens on a socket in the git folder; `bench`'s copy of the
/// vault leaves it out.
#[test]
fn bench_copies_a_vault_with_an_fsmonitor_socket() {
    let cases = "{\"q\": \"x\", \"check\": \"true\"}\n";
    let v = TestVault::with_files(BIN, &[("views/plan.md", "# Plan\n"), ("bench/cases.jsonl", cases)]);
    platform::link_tool(&v.bin(), "sh");
    let _socket = UnixListener::bind(v.root.join(".git/fsmonitor--daemon.ipc")).unwrap();
    let (code, env) = v.run(&["bench", "true"]);
    assert_eq!(code, 0, "{env:#}");
}

#[test]
fn a_write_that_cannot_be_made_changes_nothing() {
    let v = TestVault::with_files(
        BIN,
        &[("concepts/a/x.md", concept(IDS[0], "", "# X\n")), ("views/ro/v.md", "# V\n\n[[x]]\n".into())],
    );
    let ro = v.root.join("views/ro");
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
    let before = v.tree();
    let (exit, env) = v.run(&["mv", "concepts/a/x.md", "concepts/a/y.md"]);
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!((exit, error_code(&env)), (3, "io"), "{env:#}");
    assert!(error_message(&env).contains("views/ro"), "the error names the path: {env:#}");
    assert_eq!(v.tree(), before, "nothing was moved");
}

#[test]
fn a_rename_that_cannot_be_made_undoes_the_earlier_ones() {
    let v = TestVault::with_files(
        BIN,
        &[("views/x/a.md", "# A\n\n[[views/x/sub/b]]\n"), ("views/x/sub/b.md", "# B\n\n[[views/x/a]]\n")],
    );
    // `a.md` moves first; `b.md` cannot leave its read-only folder.
    let ro = v.root.join("views/x/sub");
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
    let before = v.tree();
    let (exit, env) = v.run(&["mv", "views/x", "views/y"]);
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!((exit, error_code(&env)), (3, "io"), "{env:#}");
    assert!(error_message(&env).contains("views/x/sub/b.md"), "the error names the source: {env:#}");
    assert_eq!(v.tree(), before, "the earlier move was undone");
    assert!(!v.root.join("views/y").exists(), "the folder made for the move is gone: {env:#}");
}
