//! Test helpers shared by the integration tests of `loci` and `loci-learn` (m0 §13.1):
//! fixture vaults copied to a temp dir, deterministic git, a binary run with `--json`,
//! envelope checks and a stub plugin.
//!
//! What differs between Unix and Windows is in [`platform`].

pub mod platform;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

const ULID_CHARS: &str = "[0-9A-HJKMNP-TV-Z]{26}";
static ULID: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"\b{ULID_CHARS}\b")).unwrap());
static BLOCK_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r" \^{ULID_CHARS}")).unwrap());

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// A test vault: a temp dir holding `home/`, `temp/`, `bin/` (the only `PATH` entry, with
/// `git` and optionally `loci-stub` and its `loci-stub.replies`) and `vault/`.
pub struct TestVault {
    pub tmp: tempfile::TempDir,
    pub root: PathBuf,
    /// The binary under test (`loci` or a plugin).
    pub exe: PathBuf,
}

impl TestVault {
    /// An empty vault directory (no `loci.toml`), for the binary `exe`.
    pub fn empty(exe: &str) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("vault");
        std::fs::create_dir_all(&root).unwrap();
        for dir in ["home", "temp", "bin"] {
            std::fs::create_dir_all(tmp.path().join(dir)).unwrap();
        }
        platform::link_tool(&tmp.path().join("bin"), "git");
        let root = dunce::canonicalize(&root).unwrap();
        Self { tmp, root, exe: PathBuf::from(exe) }
    }

    /// A copy of `tests/fixtures/<name>`, not yet a git repository.
    pub fn fixture(exe: &str, name: &str) -> Self {
        let v = Self::empty(exe);
        let src = fixtures_dir().join(name);
        assert!(src.join("loci.toml").is_file(), "fixture {name} has no loci.toml");
        loci_core::vault::copy_dir(&src, &v.root, &[]).unwrap();
        v
    }

    /// A copy of the fixture, `git init`ed and committed.
    pub fn committed(exe: &str, name: &str) -> Self {
        let v = Self::fixture(exe, name);
        v.init_git();
        v.commit_all("fixture");
        v
    }

    /// A committed vault holding `loci.toml` (`schema = 1`) and `files`.
    pub fn with_files(exe: &str, files: &[(impl AsRef<str>, impl AsRef<str>)]) -> Self {
        let v = Self::empty(exe);
        v.write("loci.toml", "schema = 1\n");
        for (p, t) in files {
            v.write(p.as_ref(), t.as_ref());
        }
        v.init_git();
        v.commit_all("init");
        v
    }

    /// `git init` in the vault, with no commit, listing names as their bytes on every
    /// system: macOS's default `core.precomposeUnicode` would list an NFD name as NFC,
    /// hiding `path.not-nfc`.
    pub fn init_git(&self) {
        self.git(&["init", "-q", "-b", "main"]);
        self.git(&["config", "core.precomposeUnicode", "false"]);
    }

    pub fn home(&self) -> PathBuf {
        self.tmp.path().join("home")
    }

    pub fn bin(&self) -> PathBuf {
        self.tmp.path().join("bin")
    }

    /// The test environment: nothing of the runner's but `SystemRoot`, which Windows
    /// programs need (rust-lang/rust#114737); home and temporary files inside the test.
    fn env(&self, c: &mut Command) {
        let temp = self.tmp.path().join("temp");
        c.env_clear();
        if let Some(root) = std::env::var_os("SystemRoot") {
            c.env("SystemRoot", root);
        }
        c.env("PATH", self.bin())
            .env("HOME", self.home())
            .env("USERPROFILE", self.home())
            .env("APPDATA", self.home())
            .env("TMPDIR", &temp)
            .env("TEMP", &temp)
            .env("TMP", &temp)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.org")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.org")
            .env("GIT_AUTHOR_DATE", "2025-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2025-01-01T00:00:00Z");
    }

    pub fn git(&self, args: &[&str]) -> String {
        let mut c = Command::new(self.bin().join("git"));
        self.env(&mut c);
        let out = c.arg("-C").arg(&self.root).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    }

    pub fn commit_all(&self, msg: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", msg]);
    }

    /// Runs the binary under test with `--json <args>` in the vault root. Returns (exit code, envelope).
    pub fn run(&self, args: &[&str]) -> (i32, Value) {
        self.run_in(&self.root, args)
    }

    pub fn run_in(&self, cwd: &Path, args: &[&str]) -> (i32, Value) {
        let mut c = self.command();
        c.current_dir(cwd).arg("--json").args(args);
        self.run_command(c, "")
    }

    /// Runs the binary under test with `<args>` (no implicit `--json`). Returns (exit code, stdout, stderr).
    pub fn run_raw(&self, args: &[&str]) -> (i32, String, String) {
        self.run_raw_in(&self.root, args)
    }

    pub fn run_raw_in(&self, cwd: &Path, args: &[&str]) -> (i32, String, String) {
        let mut c = self.command();
        let out = c.current_dir(cwd).args(args).output().unwrap();
        (
            out.status.code().expect("the binary exits normally"),
            String::from_utf8(out.stdout).unwrap(),
            String::from_utf8(out.stderr).unwrap(),
        )
    }

    /// The binary under test with the test environment, in the vault root. Add
    /// arguments, environment and stdin, then run it with [`TestVault::run_command`].
    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.exe);
        self.env(&mut c);
        c.current_dir(&self.root);
        c
    }

    /// Runs `c` with `stdin`; returns (exit code, envelope).
    pub fn run_command(&self, mut c: Command, stdin: &str) -> (i32, Value) {
        use std::io::Write;
        use std::process::Stdio;
        let mut child = c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        let code = out.status.code().expect("the binary exits normally");
        let stdout = String::from_utf8(out.stdout).unwrap();
        let env: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
            panic!("invalid JSON ({e}):\n{stdout}\nstderr:\n{}", String::from_utf8_lossy(&out.stderr))
        });
        check_envelope(&env, code);
        (code, env)
    }

    pub fn write(&self, rel: &str, content: impl AsRef<[u8]>) {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.root.join(rel).exists()
    }

    /// Every file of the vault (minus `.git/`) as `== path ==\ncontent`, sorted, with
    /// ULIDs redacted.
    pub fn tree(&self) -> String {
        let mut files = vec![];
        collect(&self.root, &self.root, &mut files);
        files.sort();
        let mut out = String::new();
        for rel in files {
            let bytes = std::fs::read(self.root.join(&rel)).unwrap();
            let text = match String::from_utf8(bytes) {
                Ok(t) => t,
                Err(e) => format!("<{} bytes, not UTF-8>", e.as_bytes().len()),
            };
            out.push_str(&format!("== {rel} ==\n{}\n", redact_ulids(&text)));
        }
        out
    }

    /// Installs the stub plugin `loci-stub` in the `PATH` dir, answering as `stub` says.
    pub fn install_stub(&self, stub: &Stub) {
        let exe = format!("loci-stub{}", std::env::consts::EXE_SUFFIX);
        std::fs::copy(env!("LOCI_STUB_EXE"), self.bin().join(exe)).unwrap();
        std::fs::write(self.bin().join("loci-stub.replies"), stub.replies()).unwrap();
    }

    /// Replaces the vault root (and its temp dir) in a JSON value's strings by `[vault]`
    /// / `[tmp]`, and ULIDs by `[ulid]`.
    pub fn redact(&self, v: &Value) -> Value {
        let root = self.root.to_string_lossy().into_owned();
        let tmp = dunce::canonicalize(self.tmp.path()).unwrap().to_string_lossy().into_owned();
        map_strings(v, &|s| redact_ulids(&s.replace(&root, "[vault]").replace(&tmp, "[tmp]")))
    }
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let p = entry.path();
        // `/` between names on every system, as git and loci write vault paths.
        let names = p.strip_prefix(root).unwrap().iter().map(|n| n.to_string_lossy()).collect::<Vec<_>>();
        let rel = names.join("/");
        if rel == ".git" {
            continue;
        }
        if entry.file_type().unwrap().is_dir() {
            collect(root, &p, out);
        } else {
            out.push(rel);
        }
    }
}

pub fn redact_ulids(s: &str) -> String {
    ULID.replace_all(s, "[ulid]").into_owned()
}

/// Removes every ` ^<ULID>` block id.
pub fn strip_block_ids(s: &str) -> String {
    BLOCK_ID.replace_all(s, "").into_owned()
}

/// Distinct concept ids for tests.
pub const IDS: [&str; 4] = [
    "01JB9XQ7Z3K4M5N6P7Q8R9S0A1",
    "01JB9XQ7Z3K4M5N6P7Q8R9S0A2",
    "01JB9XQ7Z3K4M5N6P7Q8R9S0A3",
    "01JB9XQ7Z3K4M5N6P7Q8R9S0A4",
];

/// A concept note: frontmatter with `id` and the `extra` lines, then `body`.
pub fn concept(id: &str, extra: &str, body: &str) -> String {
    format!("---\nid: {id}\n{extra}---\n{body}")
}

fn map_strings(v: &Value, f: &dyn Fn(&str) -> String) -> Value {
    match v {
        Value::String(s) => Value::String(f(s)),
        Value::Array(a) => Value::Array(a.iter().map(|x| map_strings(x, f)).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), map_strings(x, f))).collect()),
        other => other.clone(),
    }
}

/// Checks the envelope shape of m0 §8.
fn check_envelope(env: &Value, code: i32) {
    assert_eq!(env["schema"], 1, "{env:#}");
    assert_eq!(env["loci"], env!("CARGO_PKG_VERSION"), "{env:#}");
    assert!(env["command"].is_string(), "{env:#}");
    assert_eq!(env["ok"], code == 0, "ok iff exit 0: {env:#}");
    assert!(env["diagnostics"].is_array(), "{env:#}");
    if env["error"].is_null() {
        let errors = diags(env).iter().any(|d| d["severity"] == "error");
        assert_eq!(code != 0, errors, "without an error, exit 1 iff a diagnostic is an error: {env:#}");
    } else {
        assert!(env["data"].is_null(), "data is null with an error: {env:#}");
        assert!(env["error"]["code"].is_string() && env["error"]["message"].is_string());
    }
}

pub fn diags(env: &Value) -> &[Value] {
    env["diagnostics"].as_array().unwrap()
}

/// A diagnostic's path (`null` when it has none).
pub fn path_of(d: &Value) -> &Value {
    &d["location"]["path"]
}

/// A diagnostic's position (`null` when it has none).
pub fn span_of(d: &Value) -> &Value {
    &d["location"]["span"]
}

/// The codes of the diagnostics.
pub fn codes(env: &Value) -> Vec<String> {
    diags(env).iter().map(|d| d["code"].as_str().unwrap().to_string()).collect()
}

/// The diagnostics with a given code.
pub fn with_code<'a>(env: &'a Value, code: &str) -> Vec<&'a Value> {
    diags(env).iter().filter(|d| d["code"] == code).collect()
}

/// The messages of the diagnostics with `code`.
pub fn messages(env: &Value, code: &str) -> Vec<String> {
    with_code(env, code).iter().map(|d| d["message"].as_str().unwrap().to_string()).collect()
}

/// The envelope's error message.
pub fn error_message(env: &Value) -> &str {
    env["error"]["message"].as_str().unwrap()
}

/// The diagnostics on a given path.
pub fn on_path<'a>(env: &'a Value, path: &str) -> Vec<&'a Value> {
    diags(env).iter().filter(|d| path_of(d) == path).collect()
}

/// The codes of the diagnostics on a given path, sorted, deduplicated.
pub fn codes_on(env: &Value, path: &str) -> Vec<String> {
    let mut c: Vec<String> = on_path(env, path).iter().map(|d| d["code"].as_str().unwrap().to_string()).collect();
    c.sort();
    c.dedup();
    c
}

pub fn has(env: &Value, code: &str, path: &str) -> bool {
    diags(env).iter().any(|d| d["code"] == code && path_of(d) == path)
}

pub fn error_code(env: &Value) -> &str {
    env["error"]["code"].as_str().unwrap_or("")
}

/// The source text a diagnostic's byte range covers.
pub fn covered<'a>(text: &'a str, d: &Value) -> &'a str {
    let s = span_of(d)["byte_start"].as_u64().unwrap() as usize;
    let e = span_of(d)["byte_end"].as_u64().unwrap() as usize;
    &text[s..e]
}

/// The replies of the stub plugin (`stub/main.rs`) to its `lint` and `skills` hooks;
/// `lint` also prints [`STUB_LINT_STDERR`] on stderr. `crash` aborts the stub. Any other
/// first argument prints its arguments and environment and exits 7.
pub struct Stub {
    pub lint: String,
    pub lint_exit: i32,
    pub skills: String,
    pub skills_exit: i32,
}

pub fn envelope_json(command: &str, data: Value, diagnostics: Value, error: Value) -> String {
    serde_json::json!({
        "schema": 1, "loci": env!("CARGO_PKG_VERSION"), "command": command,
        "ok": error.is_null(), "data": data, "diagnostics": diagnostics, "error": error,
    })
    .to_string()
}

pub fn stub_diag(code: &str, path: &str) -> Value {
    serde_json::json!({
        "code": code, "severity": "warning", "location": {"path": path, "span": null},
        "message": "from the stub", "hint": null,
    })
}

pub const STUB_LINT_STDERR: &str = "stub lint went wrong";

pub const STUB_SKILL_BODY: &str = "---\nname: stub-hello\ndescription: Say hello from the stub.\n---\n# Stub hello\n";

impl Default for Stub {
    fn default() -> Self {
        Self {
            lint: envelope_json(
                "stub lint",
                Value::Null,
                serde_json::json!([stub_diag("stub.note", "concepts/a/x.md")]),
                Value::Null,
            ),
            lint_exit: 0,
            skills: envelope_json(
                "stub skills",
                serde_json::json!({"skills": [{
                    "name": "stub-hello", "description": "Say hello from the stub.", "body": STUB_SKILL_BODY,
                }]}),
                serde_json::json!([]),
                Value::Null,
            ),
            skills_exit: 0,
        }
    }
}

impl Stub {
    /// The stub's `loci-stub.replies`: one reply per line.
    fn replies(&self) -> String {
        assert!(!self.lint.contains('\n') && !self.skills.contains('\n'), "a reply is one line");
        format!("{STUB_LINT_STDERR}\n{}\n{}\n{}\n{}\n", self.lint, self.lint_exit, self.skills, self.skills_exit)
    }
}

/// One line per diagnostic: `path:line:col severity code: message (hint)`, for compact
/// snapshots.
pub fn diag_lines(env: &Value) -> String {
    let mut out = String::new();
    for d in diags(env) {
        let pos = match (span_of(d)["line"].as_u64(), span_of(d)["col"].as_u64()) {
            (Some(l), Some(c)) => format!(":{l}:{c}"),
            _ => String::new(),
        };
        out.push_str(&format!(
            "{}{pos} {} {}: {}",
            path_of(d).as_str().unwrap_or("-"),
            d["severity"].as_str().unwrap(),
            d["code"].as_str().unwrap(),
            redact_ulids(d["message"].as_str().unwrap()),
        ));
        if let Some(h) = d["hint"].as_str() {
            out.push_str(&format!(" (hint: {h})"));
        }
        out.push('\n');
    }
    out
}
