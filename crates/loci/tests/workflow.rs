//! The agent review workflow (fixture `agent-workflow`, m0 §13.2): `status`,
//! `material-dropped` and `new`.

// Unix only for now: the test vaults isolate PATH with symlinks and the stub plugin is
// a shell script (D263).
#![cfg(unix)]

mod common;

use common::*;
use insta::{assert_json_snapshot, assert_snapshot};
use serde_json::json;

// ---------------------------------------------------------------- status

#[test]
fn status_change_counts_and_inbox() {
    let v = TestVault::committed(BIN, "agent-workflow");
    v.write("concepts/a/x.md", v.read("concepts/a/x.md") + "\nMore.\n");
    std::fs::remove_file(v.root.join("concepts/a/y.md")).unwrap();
    v.write("views/staged.md", "# Staged\n");
    v.git(&["add", "views/staged.md"]);
    v.write("views/untracked.md", "# Untracked\n");
    v.write("inbox/drafts/new.md", "# New draft\n");
    let (code, env) = v.run(&["status"]);
    assert_eq!(code, 0, "{env:#}");
    assert_eq!(env["data"]["changes"], json!({"modified": 1, "deleted": 1, "added": 1, "untracked": 2}));
    assert_eq!(env["data"]["inbox"]["files"], 2);
    assert_eq!(env["data"]["inbox"]["by_folder"], json!({"drafts": 2}));
    assert_eq!(env["data"]["inbox"]["uncommitted"], json!(["inbox/drafts/new.md"]));
    let unc = with_code(&env, "inbox.uncommitted");
    assert_eq!(unc.len(), 1, "one warning, however many files");
    assert!(unc[0]["message"].as_str().unwrap().starts_with("1 inbox file(s) uncommitted"), "{env:#}");
    assert_eq!(unc[0]["severity"], "warning");
    // A modified committed draft is uncommitted too.
    v.write("inbox/drafts/draft.md", v.read("inbox/drafts/draft.md") + "\nEdit.\n");
    let (_, env) = v.run(&["status"]);
    assert_eq!(env["data"]["inbox"]["uncommitted"], json!(["inbox/drafts/draft.md", "inbox/drafts/new.md"]));
    // The human form counts by folder and lists at most ten uncommitted files.
    for n in 0..11 {
        v.write(&format!("inbox/sources/clip-{n:02}.md"), "# Clip\n");
    }
    let (_, out, _) = v.run_raw(&["status"]);
    assert!(out.contains("13 file(s), 13 uncommitted (drafts 2, sources 11)"), "{out}");
    assert_eq!(out.lines().filter(|l| l.starts_with("- inbox/")).count(), 10, "{out}");
    assert!(out.contains("- and 3 more"), "{out}");
    assert!(out.contains("Next: ask the user to commit these"), "{out}");
    // With everything committed, the next step is the intake skill.
    v.commit_all("inbox");
    let (_, out, _) = v.run_raw(&["status"]);
    assert!(out.contains("Next: process them with the intake skill (`loci skill show intake`)."), "{out}");
}

#[test]
fn status_envelope() {
    let v = TestVault::committed(BIN, "agent-workflow");
    let (code, env) = v.run(&["status"]);
    assert_eq!(code, 0);
    assert!(diags(&env).is_empty(), "{}", diag_lines(&env));
    assert_json_snapshot!(v.redact(&env), {".loci" => "[version]"});
    // The human output has every part, in order, and nothing that does not change
    // from task to task: no rules, style, index or plugins.
    let (_, out, _) = v.run_raw(&["status"]);
    for gone in ["framework rules", "# Rules", "# Style", "# Index", "## Plugins"] {
        assert!(!out.contains(gone), "{gone}:\n{out}");
    }
    let order = ["## Uncommitted changes", "## Agent work to review", "## Inbox"];
    let pos: Vec<usize> = order.iter().map(|h| out.find(h).unwrap_or_else(|| panic!("no {h}:\n{out}"))).collect();
    assert!(pos.windows(2).all(|w| w[0] < w[1]), "{out}");
    // status writes nothing.
    assert_eq!(v.git(&["status", "--porcelain"]), "");
}

/// Vault updates: a new vault starts current; one without the mark is offered every
/// update, with the procedure; `--handled` moves the mark in order; a vault ahead of
/// the binary is warned.
#[test]
fn updates_are_offered_until_handled() {
    let v = TestVault::committed(BIN, "agent-workflow");
    let newest = {
        let (_, env) = v.run(&["updates", "--all"]);
        env["data"]["updates"].as_array().unwrap().last().unwrap()["id"].as_str().unwrap().to_string()
    };
    // The fixture's loci.toml has no `updates` key: every update waits.
    let (code, out, _) = v.run_raw(&["status"]);
    assert_eq!(code, 0);
    assert!(out.contains("## Updates\n\n") && out.contains("read them with `loci updates`"), "{out}");
    let (code, out, _) = v.run_raw(&["updates"]);
    assert_eq!(code, 0);
    assert!(out.contains("1. Show the user the Offer of every waiting update") && out.contains("## u0001 · "), "{out}");
    let (code, env) = v.run(&["updates", "--handled", &newest]);
    assert_eq!((code, env["data"]["handled"].clone()), (0, json!(newest)), "{env:#}");
    assert!(v.read("loci.toml").contains(&format!("updates = \"{newest}\"")));
    let (_, env) = v.run(&["updates"]);
    assert_eq!(env["data"]["waiting"], json!([]));
    let (_, env) = v.run(&["status"]);
    assert_eq!(env["data"]["updates"], json!([]));
    // Already handled, or unknown, is refused.
    let (code, env) = v.run(&["updates", "--handled", "u0001"]);
    assert_eq!((code, error_code(&env)), (1, "updates.order"));
    let (code, env) = v.run(&["updates", "--handled", "u9999"]);
    assert_eq!((code, error_code(&env)), (1, "updates.unknown"));
    // A vault ahead of this loci: a warning in status, a refusal in updates.
    v.write("loci.toml", v.read("loci.toml").replace(&newest, "u9999"));
    let (_, env) = v.run(&["status"]);
    assert_eq!(with_code(&env, "updates.newer").len(), 1, "{env:#}");
    let (code, env) = v.run(&["updates"]);
    assert_eq!((code, error_code(&env)), (1, "updates.newer"));
    // `init` writes the newest id.
    let fresh = TestVault::empty(BIN);
    fresh.run(&["init"]);
    assert!(fresh.read("loci.toml").contains(&format!("updates = \"{newest}\"")));
}

/// The framework rules are read at session start, from `loci rules`, which needs no
/// vault; the `AGENTS.md` that `init` writes says so.
#[test]
fn rules_are_read_at_session_start() {
    let v = TestVault::empty(BIN);
    let (code, out, _) = v.run_raw(&["rules"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("# loci framework rules\n"), "{out}");
    let (_, env) = v.run(&["rules"]);
    assert_eq!(env["data"]["rules"], out);
    v.run(&["init"]);
    let agents = v.read("AGENTS.md");
    assert!(
        agents.contains("`loci rules`") && agents.contains("`RULES.md`") && agents.contains("`STYLE.md`"),
        "{agents}"
    );
}

/// When git cannot list the vault, no command runs on a guess (m0 §3).
#[test]
fn a_vault_git_cannot_list_is_refused() {
    let v = TestVault::committed(BIN, "agent-workflow");
    std::fs::write(v.root.join(".git/index"), "corrupt").unwrap();
    for args in [&["status"][..], &["lint"], &["new", "concepts/a/fresh"]] {
        let (code, env) = v.run(args);
        assert_eq!((code, error_code(&env)), (3, "git"), "{args:?}: {env:#}");
    }
}

#[test]
fn status_checks_agents_md_but_not_skills() {
    // Skills are managed outside loci: a vault without skill folders gets no warning.
    let v = TestVault::committed(BIN, "agent-workflow");
    let (_, env) = v.run(&["status"]);
    assert!(with_code(&env, "install.missing").is_empty(), "{}", diag_lines(&env));
    std::fs::remove_file(v.root.join("AGENTS.md")).unwrap();
    let (_, env) = v.run(&["status"]);
    let missing = with_code(&env, "install.missing");
    assert_eq!(missing.len(), 1, "{}", diag_lines(&env));
    assert!(missing[0]["message"].as_str().unwrap().contains("AGENTS.md"));
}

// ---------------------------------------------------------------- material-dropped

#[test]
fn material_dropped_when_a_deleted_draft_is_not_absorbed() {
    let v = TestVault::committed(BIN, "agent-workflow");
    std::fs::remove_file(v.root.join("inbox/drafts/draft.md")).unwrap();
    let (code, env) = v.run(&["lint"]);
    assert_eq!(code, 0, "warnings only: {}", diag_lines(&env));
    let dropped = with_code(&env, "material-dropped");
    assert_eq!(dropped.len(), 3, "{}", diag_lines(&env));
    // Warnings, each on the deleted path (§7.1: vault-level rules report on the first
    // involved path).
    assert!(dropped.iter().all(|d| d["severity"] == "warning" && path_of(d) == "inbox/drafts/draft.md"));
    assert_snapshot!(messages(&env, "material-dropped").join("\n"));
}

#[test]
fn material_dropped_silent_when_absorbed() {
    let v = TestVault::committed(BIN, "agent-workflow");
    let draft = v.read("inbox/drafts/draft.md");
    std::fs::remove_file(v.root.join("inbox/drafts/draft.md")).unwrap();
    let body = draft.strip_prefix("# Draft\n").unwrap();
    v.write("concepts/a/x.md", v.read("concepts/a/x.md") + body);
    let (_, env) = v.run(&["lint"]);
    assert!(messages(&env, "material-dropped").is_empty(), "{}", diag_lines(&env));
    // Partly absorbed: only the code block is kept.
    let v = TestVault::committed(BIN, "agent-workflow");
    std::fs::remove_file(v.root.join("inbox/drafts/draft.md")).unwrap();
    v.write("views/kept.md", "# Kept\n\n```python\nprint(\"hi\")\n```\n");
    let (_, env) = v.run(&["lint"]);
    let msgs = messages(&env, "material-dropped");
    assert_eq!(msgs.len(), 2, "{}", diag_lines(&env));
    assert!(msgs.iter().all(|m| !m.contains("code")), "{msgs:?}");
}

#[test]
fn material_dropped_skipped_without_head() {
    let v = TestVault::fixture(BIN, "agent-workflow");
    v.init_git();
    std::fs::remove_file(v.root.join("inbox/drafts/draft.md")).unwrap();
    let (code, env) = v.run(&["lint"]);
    assert_eq!(code, 0, "{}", diag_lines(&env));
    assert!(messages(&env, "material-dropped").is_empty());
}

// ---------------------------------------------------------------- new

#[test]
fn new_writes_the_skeleton() {
    let v = TestVault::committed(BIN, "agent-workflow");
    let (code, env) = v.run(&["new", "concepts/a/sub/fresh"]);
    assert_eq!(code, 0, "{env:#}");
    assert_eq!(env["command"], "new");
    assert_eq!(env["data"]["path"], "concepts/a/sub/fresh.md");
    let id = env["data"]["id"].as_str().unwrap();
    assert!(id.parse::<loci_core::id::Ulid>().is_ok(), "{id}");
    let text = v.read("concepts/a/sub/fresh.md");
    assert_eq!(text, format!("---\nid: {id}\nprereqs: []\n---\n# fresh\n"));
    assert_json_snapshot!(v.redact(&env), {".loci" => "[version]"});
    // Marked intent-to-add, so `git diff` shows it.
    assert_eq!(v.git(&["status", "--porcelain", "--", "concepts/a/sub/fresh.md"]), " A concepts/a/sub/fresh.md\n");
    // A slug already taken names the note that has it.
    let (_, env) = v.run(&["new", "concepts/b/fresh"]);
    assert_eq!(env["error"]["message"], "`fresh` is already the slug of `concepts/a/sub/fresh.md`");
    // The skeleton lints clean.
    let (_, env) = v.run(&["lint"]);
    assert_eq!(codes_on(&env, "concepts/a/sub/fresh.md"), Vec::<String>::new());

    let (code, env) = v.run(&["new", "concepts/a/titled.md", "--title", "A Title"]);
    assert_eq!(code, 0);
    assert!(v.read("concepts/a/titled.md").ends_with("---\n# A Title\n"));
    assert_ne!(env["data"]["id"], id);
}

#[test]
fn new_refusals() {
    let v = TestVault::committed(BIN, "agent-workflow");
    let before = v.tree();
    for (path, code) in [
        ("concepts/a/x.md", "new.exists"),
        ("concepts/a/Bad_Slug", "slug.format"),
        ("concepts/b/x", "new.collision"),
        ("concepts/b/EX", "slug.format"),
        ("views/x", "new.no-area"),
        ("concepts/top", "new.no-area"),
        ("journal/r/2025/01/25-01-01", "new.no-area"),
        // The destination checks `mv` makes too.
        ("concepts/A/y", "path.case-twin"),
        // Under a file, which no folder can be made at.
        ("concepts/a/x.md/y", "new.exists"),
    ] {
        let (exit, env) = v.run(&["new", path]);
        assert_eq!((exit, error_code(&env)), (1, code), "new {path}: {env:#}");
    }
    let (exit, env) = v.run(&["new", "concepts/a/sharp", "--title", "C #"]);
    assert_eq!(exit, 0, "{env:#}");
    assert!(v.read("concepts/a/sharp.md").ends_with("---\n# C # #\n"), "the title reads back whole");
    v.git(&["rm", "-q", "--cached", "concepts/a/sharp.md"]);
    std::fs::remove_file(v.root.join("concepts/a/sharp.md")).unwrap();
    for title in ["", " ", "Two\n===", "a\rb"] {
        let (exit, env) = v.run(&["new", "concepts/a/titled", "--title", title]);
        assert_eq!((exit, error_code(&env)), (1, "new.title"), "--title {title:?}: {env:#}");
    }
    assert_eq!(v.tree(), before);
    // Said as `mv` says it: an area file is in an area, but not a concept.
    let (_, env) = v.run(&["new", "concepts/a/AREA"]);
    assert_eq!(error_code(&env), "new.no-area");
    assert!(error_message(&env).ends_with("would be the area file, not a concept"), "{env:#}");
    v.write(".gitignore", "concepts/a/secret.md\n");
    v.commit_all("ignore");
    let (exit, env) = v.run(&["new", "concepts/a/secret"]);
    assert_eq!((exit, error_code(&env)), (1, "path.ignored"), "{env:#}");
}

// ---------------------------------------------------------------- done

#[test]
fn done_records_the_agent_work_without_touching_the_index() {
    let v = TestVault::committed(BIN, "agent-workflow");
    let head = v.git(&["rev-parse", "HEAD"]).trim().to_string();
    let reference = format!("refs/loci/agent/{head}");
    // Nothing changed: nothing recorded.
    let (code, env) = v.run(&["done"]);
    assert_eq!(code, 0, "{env:#}");
    assert!(env["data"]["record"].is_null());
    // The agent edits a note, creates one, and leaves an ignored file.
    v.write("concepts/a/x.md", v.read("concepts/a/x.md") + "\nAgent text.\n");
    v.write("views/new.md", "# New\n");
    v.write(".gitignore", v.read(".gitignore") + "scratch.txt\n");
    v.write("scratch.txt", "ignored");
    let status_before = v.git(&["status", "--porcelain"]);
    let (code, env) = v.run(&["done"]);
    assert_eq!(code, 0, "{env:#}");
    assert_eq!(env["data"]["record"], reference);
    assert!(env["data"]["files"].is_number(), "the lint data is kept");
    assert_eq!(v.git(&["status", "--porcelain"]), status_before, "the working tree and index are untouched");
    assert_eq!(v.git(&["diff", "--cached", "--name-only"]), "");
    assert_eq!(v.git(&["rev-parse", &format!("{reference}^")]).trim(), head, "the parent is the starting commit");
    let changed = v.git(&["diff", "--name-only", &head, &reference]);
    assert_eq!(changed, ".gitignore\nconcepts/a/x.md\nviews/new.md\n");
    // A second run on the same base replaces the record.
    v.write("views/new.md", "# New, revised\n");
    let (_, env) = v.run(&["done"]);
    assert_eq!(env["data"]["record"], reference);
    assert_eq!(v.git(&["for-each-ref", "--format=%(refname)", "refs/loci/agent/"]).lines().count(), 1);
    assert_eq!(v.git(&["show", &format!("{reference}:views/new.md")]), "# New, revised\n");
    // `loci status` counts the records.
    let (_, env) = v.run(&["status"]);
    assert_eq!(env["data"]["agent_records"], 1);
}

/// The agent's notes on what was hard go into the record, carried across runs on the
/// same base, and are recorded even when nothing changed.
#[test]
fn done_keeps_the_agents_notes() {
    let v = TestVault::committed(BIN, "agent-workflow");
    let head = v.git(&["rev-parse", "HEAD"]).trim().to_string();
    let reference = format!("refs/loci/agent/{head}");
    let message = || v.git(&["log", "-1", "--format=%B", &reference]);
    // Nothing changed, but a note: recorded.
    let (code, env) = v.run(&["done", "--note", "rg found nothing for FIFO"]);
    assert_eq!((code, env["data"]["record"].clone()), (0, json!(reference)), "{env:#}");
    assert_eq!(message(), "loci done: the agent's work\n\n- rg found nothing for FIFO\n\n");
    // A later run adds its notes to those already there, once each.
    v.write("views/new.md", "# New\n");
    v.run(&["done", "--note", "unclear  when to\ncreate a sub-folder", "--note", "rg found nothing for FIFO"]);
    assert_eq!(
        message(),
        "loci done: the agent's work\n\n- rg found nothing for FIFO\n- unclear when to create a sub-folder\n\n"
    );
    // A run without notes keeps them.
    v.run(&["done"]);
    assert!(message().contains("- unclear when to create a sub-folder"), "{}", message());
}

#[test]
fn done_without_a_commit_warns() {
    let v = TestVault::fixture(BIN, "agent-workflow");
    v.init_git();
    let (code, env) = v.run(&["done", "--note", "hard"]);
    assert_eq!((code, env["data"]["record"].clone()), (0, json!(null)), "{env:#}");
    assert_eq!(messages(&env, "done.record").len(), 1, "the note is not lost silently: {env:#}");
}

#[test]
fn done_prunes_old_records() {
    let v = TestVault::committed(BIN, "agent-workflow");
    // The test git is dated 2025-01-01, long before now.
    let old = v.git(&["commit-tree", "HEAD^{tree}", "-p", "HEAD", "-m", "old"]).trim().to_string();
    v.git(&["update-ref", "refs/loci/agent/old", &old]);
    v.write("views/new.md", "# New\n");
    let (code, env) = v.run(&["done"]);
    assert_eq!(code, 0, "{env:#}");
    let refs = v.git(&["for-each-ref", "--format=%(refname)", "refs/loci/agent/"]);
    assert_eq!(refs.trim(), env["data"]["record"].as_str().unwrap(), "only the new record remains");
}

// ---------------------------------------------------------------- working directory

/// A working directory deleted under loci is an `io` refusal, not a panic.
#[test]
fn deleted_working_directory_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let gone = tmp.path().join("gone");
    std::fs::create_dir(&gone).unwrap();
    let out = std::process::Command::new("sh")
        .current_dir(&gone)
        .args(["-c", "rmdir \"$PWD\" && exec \"$0\" --json lint", BIN])
        .output()
        .unwrap();
    let env: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| panic!("{out:?}"));
    assert_eq!(out.status.code(), Some(3), "{env:#}");
    assert_eq!(env["error"]["code"], "io", "{env:#}");
}

#[test]
fn closed_stdout_is_no_crash() {
    // A reader that stops early (`loci status | head -0`) closes the pipe first.
    let v = TestVault::committed(BIN, "agent-workflow");
    let mut c = v.command();
    let mut child = c.arg("status").stdout(std::process::Stdio::piped()).spawn().unwrap();
    drop(child.stdout.take());
    assert_eq!(child.wait().unwrap().code(), Some(0));
}

#[test]
fn closed_stderr_is_no_crash() {
    // `loci lint 2>&1 | head` closes the pipe the diagnostics and a refusal go to.
    let v = TestVault::committed(BIN, "agent-workflow");
    let mut c = v.command();
    let mut child =
        c.args(["mv", "concepts/none.md", "concepts/other.md"]).stderr(std::process::Stdio::piped()).spawn().unwrap();
    drop(child.stderr.take());
    assert_eq!(child.wait().unwrap().code(), Some(1));
}
