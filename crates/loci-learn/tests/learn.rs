//! Integration tests for `loci-learn` (learn-plugin §5), run against temporary vaults.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const ID1: &str = "01JBA2K7N4P8R3T6W9X2Z5C8DF";
const ID2: &str = "01JB9XR2M8C4V6T0Q3W5H7K9PA";
const CONCEPT: &str = "01JB9XQ7Z3K4M5N6P7Q8R9S0TV";

struct Vault {
    dir: tempfile::TempDir,
}

impl Vault {
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault { dir };
        v.write("loci.toml", "schema = 1\n");
        for (p, t) in files {
            v.write(p, t);
        }
        v.git(&["init", "-q"]);
        v.git(&["add", "-A"]);
        v.git(&["commit", "-qm", "init"]);
        v
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn write(&self, path: &str, text: &str) {
        let p = self.root().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root().join(path)).unwrap_or_default()
    }

    fn git_env(&self, c: &mut Command) {
        c.env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("HOME", self.root())
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t");
    }

    fn git(&self, args: &[&str]) -> String {
        let mut c = Command::new("git");
        c.args(args).current_dir(self.root());
        self.git_env(&mut c);
        let out = c.output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    }

    fn learn(&self, args: &[&str], stdin: &str) -> (Output, Value) {
        self.learn_with(args, stdin, Some(VERSION))
    }

    fn learn_with(&self, args: &[&str], stdin: &str, version: Option<&str>) -> (Output, Value) {
        let mut c = Command::new(bin());
        c.args(args).arg("--json").current_dir(self.root()).env("LOCI_VAULT", self.root());
        match version {
            Some(v) => c.env("LOCI_VERSION", v),
            None => c.env_remove("LOCI_VERSION"),
        };
        self.git_env(&mut c);
        c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = c.spawn().unwrap();
        child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        let env: Value = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("bad envelope ({e}): {}", String::from_utf8_lossy(&out.stdout)));
        (out, env)
    }
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_loci-learn"))
}

fn concept(id: &str, cards: &str) -> String {
    format!("---\nid: {id}\ndigest: d\n---\n# T\n\nBody.\n\n## Cards\n\n{cards}")
}

fn codes(env: &Value) -> Vec<String> {
    env["diagnostics"].as_array().unwrap().iter().map(|d| d["code"].as_str().unwrap().to_string()).collect()
}

fn error_code(env: &Value) -> String {
    env["error"]["code"].as_str().unwrap_or("").to_string()
}

#[test]
fn version_and_direct_runs() {
    let v = Vault::new(&[]);
    let (out, env) = v.learn_with(&["lint"], "", None);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(error_code(&env), "plugin.direct");
    let (_, env) = v.learn_with(&["lint"], "", Some("99.0.0"));
    assert_eq!(error_code(&env), "plugin.version");
}

#[test]
fn cards_lint() {
    let v = Vault::new(&[
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
    let (out, env) = v.learn(&["lint"], "");
    assert_eq!(out.status.code(), Some(1));
    assert!(env["data"].is_null());
    let mut c = codes(&env);
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
    let v = Vault::new(&[("concepts/a/x.md", &concept(CONCEPT, &format!("- q :: a ^{ID1}\n")))]);
    v.write("views/draft.md", "# Uncommitted\n");
    let (out, env) = v.learn(&["log"], r#"{"card":"X","purpose":"pretest","grader":"self","grade":"good"}"#);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(error_code(&env), "learn.bad-input");

    let input =
        format!(r#"{{"card":"{ID1}","purpose":"practice","grader":"llm","verdicts":[[true,true],[true,false]]}}"#);
    let (out, env) = v.learn(&["log"], &input);
    assert_eq!(out.status.code(), Some(0), "{env}");
    assert_eq!(env["data"]["line"]["grade"], "hard");
    assert_eq!(v.read("plugins/learn/.gitattributes"), "reviews.jsonl merge=union\n");
    let files = v.git(&["show", "--name-only", "--format=", "HEAD"]);
    assert_eq!(files, "plugins/learn/.gitattributes\nplugins/learn/reviews.jsonl\n");
    assert!(v.git(&["status", "--porcelain", "-uall"]).contains("views/draft.md"), "other changes stay");

    let bad = [
        (format!(r#"{{"card":"{ID1}","purpose":"review","grader":"llm","grade":"good","verdicts":[[true]]}}"#), "learn.bad-input"),
        (r#"{"card":"01JBXXXXXXXXXXXXXXXXXXXXXX","purpose":"review","grader":"self","grade":"good"}"#.to_string(), "learn.unknown-card"),
        (r#"{"concept":"01JBXXXXXXXXXXXXXXXXXXXXXX","item":"q","purpose":"diagnostic","grader":"self","grade":"good"}"#.to_string(), "learn.unknown-concept"),
    ];
    for (input, code) in bad {
        let (_, env) = v.learn(&["log"], &input);
        assert_eq!(error_code(&env), code, "{input}");
    }
    assert_eq!(v.read("plugins/learn/reviews.jsonl").lines().count(), 1);
}

#[test]
fn state_kinds() {
    let v = Vault::new(&[
        ("concepts/a/x.md", &concept(CONCEPT, &format!("- q :: a ^{ID1}\n- q2 :: a ^{ID2}\n"))),
        ("concepts/a/y.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TW\ndigest: d\n---\n# Y\n"),
    ]);
    let (_, env) = v.learn(&["state", "x"], "");
    assert_eq!(env["data"]["estimate"]["kind"], "unknown");
    v.learn(&["log"], &format!(r#"{{"card":"{ID1}","purpose":"review","grader":"self","grade":"good"}}"#));
    let (_, env) = v.learn(&["state", "x"], "");
    assert_eq!(env["data"]["estimate"]["kind"], "cards");
    assert_eq!(env["data"]["lines"].as_array().unwrap().len(), 1, "only reviewed cards count");
    v.learn(&["log"], r#"{"concept":"01JB9XQ7Z3K4M5N6P7Q8R9S0TW","item":"q?","purpose":"diagnostic","grader":"self","grade":"again"}"#);
    let (_, env) = v.learn(&["state", "y"], "");
    assert_eq!(env["data"]["estimate"]["kind"], "item");
    assert_eq!(env["data"]["estimate"]["grade"], "again");
    let (_, env) = v.learn(&["state", "nope"], "");
    assert_eq!(error_code(&env), "learn.unknown-concept");
}

#[test]
fn review_session_and_config() {
    let v = Vault::new(&[("concepts/a/x.md", &concept(CONCEPT, &format!("- q1 :: a1 ^{ID1}\n- q2 :: a2 ^{ID2}\n")))]);
    let notes_before = v.read("concepts/a/x.md");
    // No grades: nothing committed, exit 0.
    let head = v.git(&["rev-parse", "HEAD"]);
    let (out, env) = v.learn(&["review"], "q\n");
    assert_eq!(out.status.code(), Some(0), "{env}");
    assert!(env["data"]["commit"].is_null());
    assert_eq!(v.git(&["rev-parse", "HEAD"]), head);
    // Two new cards graded; one commit at the end.
    let (out, env) = v.learn(&["review"], "\ngood\n\neasy\n");
    assert_eq!(out.status.code(), Some(0), "{env}");
    assert_eq!(env["data"]["reviewed"], 2);
    assert_eq!(env["data"]["new"], 2);
    assert_eq!(v.read("plugins/learn/reviews.jsonl").lines().count(), 2);
    assert_eq!(v.git(&["rev-list", "--count", "HEAD"]).trim(), "2");
    assert_eq!(v.read("concepts/a/x.md"), notes_before, "no note is written");
    // Both were introduced today and are not yet due: nothing to review.
    let (_, env) = v.learn(&["review"], "");
    assert_eq!(env["data"]["reviewed"], 0);
    assert_eq!(env["data"]["due_left"], 0);
    // Config: new_per_day limits new cards; bad values are learn.config.
    v.write("plugins/learn/config.toml", "new_per_day = -1\n");
    let (_, env) = v.learn(&["review"], "");
    assert_eq!(error_code(&env), "learn.config");
}

#[test]
fn new_per_day_limit() {
    let v = Vault::new(&[("concepts/a/x.md", &concept(CONCEPT, &format!("- q1 :: a1 ^{ID1}\n- q2 :: a2 ^{ID2}\n")))]);
    v.write("plugins/learn/config.toml", "new_per_day = 1\n");
    let (_, env) = v.learn(&["review"], "\ngood\n");
    assert_eq!(env["data"]["new"], 1);
    assert_eq!(env["data"]["reviewed"], 1);
}

#[test]
fn deleted_card_lines_are_ignored() {
    let v = Vault::new(&[("concepts/a/x.md", &concept(CONCEPT, &format!("- q1 :: a1 ^{ID1}\n")))]);
    v.write("plugins/learn/reviews.jsonl", r#"{"t":"2020-01-01T00:00:00Z","card":"01JBGONEGONEGONEGONEGONEGO","purpose":"review","grader":"self","grade":"good"}"#);
    v.write("plugins/learn/reviews.jsonl", &(v.read("plugins/learn/reviews.jsonl") + "\n"));
    let (_, env) = v.learn(&["review"], "q\n");
    assert_eq!(env["data"]["new"], 1, "{env}");
}
