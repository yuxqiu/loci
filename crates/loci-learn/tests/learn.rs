//! Integration tests for `loci-learn` (learn-plugin §5), run against temporary vaults.

use loci_testkit::{TestVault, diags, error_code};
use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_loci-learn");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const ID1: &str = "01JBA2K7N4P8R3T6W9X2Z5C8DF";
const ID2: &str = "01JB9XR2M8C4V6T0Q3W5H7K9PA";
const CONCEPT: &str = "01JB9XQ7Z3K4M5N6P7Q8R9S0TV";

fn vault(files: &[(&str, &str)]) -> TestVault {
    TestVault::with_files(BIN, files)
}

/// Runs `loci-learn <args> --json` as `loci` would, with `stdin`.
fn learn(v: &TestVault, args: &[&str], stdin: &str) -> (i32, Value) {
    learn_with(v, args, stdin, Some(VERSION))
}

/// As [`learn`], with `LOCI_VERSION` set to `version` or unset.
fn learn_with(v: &TestVault, args: &[&str], stdin: &str, version: Option<&str>) -> (i32, Value) {
    let mut c = v.command();
    c.args(args).arg("--json").env("LOCI_VAULT", &v.root);
    if let Some(version) = version {
        c.env("LOCI_VERSION", version);
    }
    v.run_command(c, stdin)
}

/// A concept note whose `## Cards` section holds `cards`.
fn concept(id: &str, cards: &str) -> String {
    loci_testkit::concept(id, "", &format!("# T\n\nBody.\n\n## Cards\n\n{cards}"))
}

fn diag_codes(env: &Value) -> Vec<String> {
    diags(env).iter().map(|d| d["code"].as_str().unwrap().to_string()).collect()
}

#[test]
fn version_and_direct_runs() {
    let v = vault(&[]);
    let (code, env) = learn_with(&v, &["lint"], "", None);
    assert_eq!(code, 1);
    assert_eq!(error_code(&env), "plugin.direct");
    let (_, env) = learn_with(&v, &["lint"], "", Some("99.0.0"));
    assert_eq!(error_code(&env), "plugin.version");
}

#[test]
fn cards_lint() {
    let v = vault(&[
        (
            "concepts/a/x.md",
            &concept(
                CONCEPT,
                &format!(
                    "- tight :: a ^{ID1}\n- `std::vector` q :: a\n- no separator\n- short :: a ^abc\n\n## Cards\n\n- second :: section\n"
                ),
            ),
        ),
        ("concepts/a/y.md", &concept("01JB9XQ7Z3K4M5N6P7Q8R9S0TW", &format!("- dup :: a ^{ID1}\n"))),
    ]);
    let before = v.git(&["status", "--porcelain"]);
    let (code, env) = learn(&v, &["lint"], "");
    assert_eq!(code, 1);
    assert!(env["data"].is_null());
    let mut c = diag_codes(&env);
    c.sort();
    assert_eq!(
        c,
        vec![
            "learn.card-format",
            "learn.card-format",
            "learn.id-duplicate",
            "learn.id-form",
            "learn.no-id",
            "learn.no-id"
        ],
        "{env}"
    );
    let no_id = env["diagnostics"].as_array().unwrap().iter().find(|d| d["code"] == "learn.no-id").unwrap();
    assert!(no_id["hint"].as_str().unwrap().contains("block-ids"));
    assert_eq!(v.git(&["status", "--porcelain"]), before, "lint must write nothing");
}

#[test]
fn log_commits_alone_and_validates() {
    let v = vault(&[("concepts/a/x.md", &concept(CONCEPT, &format!("- q :: a ^{ID1}\n")))]);
    v.write("views/draft.md", "# Uncommitted\n");
    let (code, env) = learn(&v, &["log"], r#"{"card":"X","grade":"great"}"#);
    assert_eq!(code, 1);
    assert_eq!(error_code(&env), "learn.bad-input");

    let input = format!(r#"{{"card":"{ID1}","grade":"hard"}}"#);
    let (code, env) = learn(&v, &["log"], &input);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["line"]["grade"], "hard");
    assert_eq!(v.read("plugins/learn/.gitattributes"), "reviews.jsonl merge=union\n");
    let files = v.git(&["show", "--name-only", "--format=", "HEAD"]);
    assert_eq!(files, "plugins/learn/.gitattributes\nplugins/learn/reviews.jsonl\n");
    assert!(v.git(&["status", "--porcelain", "-uall"]).contains("views/draft.md"), "other changes stay");

    let bad = [
        (format!(r#"{{"card":"{ID1}","concept":"{CONCEPT}","grade":"good"}}"#), "learn.bad-input"),
        (r#"{"card":"01JBXXXXXXXXXXXXXXXXXXXXXX","grade":"good"}"#.to_string(), "learn.unknown-card"),
        (r#"{"concept":"01JBXXXXXXXXXXXXXXXXXXXXXX","grade":"good"}"#.to_string(), "learn.unknown-concept"),
    ];
    for (input, code) in bad {
        let (_, env) = learn(&v, &["log"], &input);
        assert_eq!(error_code(&env), code, "{input}");
    }
    assert_eq!(v.read("plugins/learn/reviews.jsonl").lines().count(), 1);

    // A log whose last line lost its line break still takes a new line of its own.
    let log = v.read("plugins/learn/reviews.jsonl");
    v.write("plugins/learn/reviews.jsonl", log.trim_end());
    let (code, env) = learn(&v, &["log"], &input);
    assert_eq!(code, 0, "{env}");
    assert_eq!(v.read("plugins/learn/reviews.jsonl").lines().count(), 2);
    let (code, env) = learn(&v, &["state", "x"], "");
    assert_eq!(code, 0, "the log still parses: {env}");
}

#[cfg(unix)]
#[test]
fn a_failed_commit_leaves_nothing_staged() {
    use std::os::unix::fs::PermissionsExt;
    let v = vault(&[("concepts/a/x.md", &concept(CONCEPT, &format!("- q :: a ^{ID1}\n")))]);
    let hook = v.root.join(".git/hooks/pre-commit");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (code, env) = learn(&v, &["log"], &format!(r#"{{"card":"{ID1}","grade":"hard"}}"#));
    assert_eq!((code, error_code(&env)), (3, "git"), "{env}");
    assert_eq!(v.git(&["diff", "--cached", "--name-only"]), "", "the removed line is not left staged");
}

#[test]
fn a_failed_append_names_the_file() {
    use std::os::unix::fs::PermissionsExt;
    let v = vault(&[("concepts/a/x.md", &concept(CONCEPT, &format!("- q :: a ^{ID1}\n")))]);
    let dir = v.root.join("plugins/learn");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let (code, env) = learn(&v, &["log"], &format!(r#"{{"card":"{ID1}","grade":"hard"}}"#));
    assert_eq!((code, error_code(&env)), (3, "io"), "{env}");
    let message = env["error"]["message"].as_str().unwrap();
    assert!(message.contains(&dir.join(".gitattributes").display().to_string()), "{message}");
}

#[test]
fn the_log_is_not_written_through_a_symlink() {
    let v = vault(&[("concepts/a/x.md", &concept(CONCEPT, &format!("- q :: a ^{ID1}\n")))]);
    let outside = v.root.parent().unwrap().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::create_dir_all(v.root.join("plugins")).unwrap();
    std::os::unix::fs::symlink(&outside, v.root.join("plugins/learn")).unwrap();
    // Ignored, so opening the vault does not see it.
    std::fs::write(v.root.join(".git/info/exclude"), "/plugins/learn\n").unwrap();
    let (code, env) = learn(&v, &["log"], &format!(r#"{{"card":"{ID1}","grade":"hard"}}"#));
    assert_eq!((code, error_code(&env)), (1, "file.symlink"), "{env}");
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0, "nothing written outside the vault");
    std::fs::write(outside.join("config.toml"), "new_per_day = 1\n").unwrap();
    for args in [&["state", "x"][..], &["review"]] {
        let (code, env) = learn(&v, args, "");
        assert_eq!((code, error_code(&env)), (1, "file.symlink"), "{args:?} reads nothing through it: {env}");
    }
}

#[test]
fn state_kinds() {
    let v = vault(&[
        ("concepts/a/x.md", &concept(CONCEPT, &format!("- q :: a ^{ID1}\n- q2 :: a ^{ID2}\n"))),
        ("concepts/a/y.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TW\n---\n# Y\n"),
    ]);
    let (_, env) = learn(&v, &["state", "x"], "");
    assert_eq!(env["data"]["estimate"]["kind"], "unknown");
    learn(&v, &["log"], &format!(r#"{{"card":"{ID1}","grade":"good"}}"#));
    let (_, env) = learn(&v, &["state", "x"], "");
    assert_eq!(env["data"]["estimate"]["kind"], "cards");
    assert_eq!(env["data"]["lines"].as_array().unwrap().len(), 1, "only reviewed cards count");
    learn(&v, &["log"], r#"{"concept":"01JB9XQ7Z3K4M5N6P7Q8R9S0TW","grade":"again"}"#);
    let (_, env) = learn(&v, &["state", "y"], "");
    assert_eq!(env["data"]["estimate"]["kind"], "item");
    assert_eq!(env["data"]["estimate"]["grade"], "again");
    let (_, env) = learn(&v, &["state", "nope"], "");
    assert_eq!(error_code(&env), "note.unknown");
}

#[test]
fn review_session_and_config() {
    let v = vault(&[("concepts/a/x.md", &concept(CONCEPT, &format!("- q1 :: a1 ^{ID1}\n- q2 :: a2 ^{ID2}\n")))]);
    let notes_before = v.read("concepts/a/x.md");
    // No grades: nothing committed, exit 0.
    let head = v.git(&["rev-parse", "HEAD"]);
    let (code, env) = learn(&v, &["review"], "q\n");
    assert_eq!(code, 0, "{env}");
    assert!(env["data"]["commit"].is_null());
    assert_eq!(v.git(&["rev-parse", "HEAD"]), head);
    // Two new cards graded; one commit at the end.
    let (code, env) = learn(&v, &["review"], "\ngood\n\neasy\n");
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["reviewed"], 2);
    assert_eq!(env["data"]["new"], 2);
    assert_eq!(v.read("plugins/learn/reviews.jsonl").lines().count(), 2);
    assert_eq!(v.git(&["rev-list", "--count", "HEAD"]).trim(), "2");
    assert_eq!(v.read("concepts/a/x.md"), notes_before, "no note is written");
    // Both were introduced today and are not yet due: nothing to review.
    let (_, env) = learn(&v, &["review"], "");
    assert_eq!(env["data"]["reviewed"], 0);
    assert_eq!(env["data"]["due_left"], 0);
    // Config: new_per_day limits new cards; bad values are learn.config.
    v.write("plugins/learn/config.toml", "new_per_day = -1\n");
    let (_, env) = learn(&v, &["review"], "");
    assert_eq!(error_code(&env), "learn.config");
}

#[test]
fn new_per_day_limit() {
    let v = vault(&[("concepts/a/x.md", &concept(CONCEPT, &format!("- q1 :: a1 ^{ID1}\n- q2 :: a2 ^{ID2}\n")))]);
    v.write("plugins/learn/config.toml", "new_per_day = 1\n");
    let (_, env) = learn(&v, &["review"], "\ngood\n");
    assert_eq!(env["data"]["new"], 1);
    assert_eq!(env["data"]["reviewed"], 1);
}

#[test]
fn deleted_card_lines_are_ignored() {
    let v = vault(&[("concepts/a/x.md", &concept(CONCEPT, &format!("- q1 :: a1 ^{ID1}\n")))]);
    v.write(
        "plugins/learn/reviews.jsonl",
        "{\"t\":\"2020-01-01T00:00:00Z\",\"card\":\"01JBZZZZZZZZZZZZZZZZZZZZZZ\",\"grade\":\"good\"}\n",
    );
    let (_, env) = learn(&v, &["review"], "q\n");
    assert_eq!(env["data"]["new"], 1, "{env}");
}

#[test]
fn grades_left_uncommitted_by_an_interrupted_review_are_committed_by_the_next_log() {
    let v = vault(&[("concepts/a/x.md", &concept(CONCEPT, &format!("- q1 :: a1 ^{ID1}\n")))]);
    // An interrupted review leaves its appended lines uncommitted.
    let (code, _) = learn(&v, &["review"], "\ngood\nq\n");
    assert_eq!(code, 0);
    v.write(
        "plugins/learn/reviews.jsonl",
        &(v.read("plugins/learn/reviews.jsonl")
            + &format!("{{\"t\":\"2026-01-01T00:00:00Z\",\"card\":\"{ID1}\",\"grade\":\"easy\"}}\n")),
    );
    let (code, env) = learn(&v, &["log"], &format!("{{\"card\":\"{ID1}\",\"grade\":\"hard\"}}"));
    assert_eq!(code, 0, "{env}");
    assert_eq!(v.git(&["status", "--porcelain", "--", "plugins/learn"]), "", "the whole log is committed");
}
