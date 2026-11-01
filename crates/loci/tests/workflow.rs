//! The agent review workflow (fixture `agent-workflow`, m0 §13.2): `status`,
//! `material-dropped` and `new`.

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
    assert_eq!(env["data"]["inbox"]["files"], json!(["inbox/drafts/draft.md", "inbox/drafts/new.md"]));
    assert_eq!(env["data"]["inbox"]["uncommitted"], json!(["inbox/drafts/new.md"]));
    let unc = with_code(&env, "inbox.uncommitted");
    assert_eq!(unc.len(), 1);
    assert_eq!(unc[0]["path"], "inbox/drafts/new.md");
    assert_eq!(unc[0]["severity"], "warning");
    // A modified committed draft is uncommitted too.
    v.write("inbox/drafts/draft.md", v.read("inbox/drafts/draft.md") + "\nEdit.\n");
    let (_, env) = v.run(&["status"]);
    assert_eq!(env["data"]["inbox"]["uncommitted"], json!(["inbox/drafts/draft.md", "inbox/drafts/new.md"]));
}

#[test]
fn status_envelope() {
    let v = TestVault::committed(BIN, "agent-workflow");
    let (code, env) = v.run(&["status"]);
    assert_eq!(code, 0);
    assert!(diags(&env).is_empty(), "{}", diag_lines(&env));
    assert_eq!(env["data"]["rules"]["RULES.md"], v.read("RULES.md"));
    assert_eq!(env["data"]["rules"]["STYLE.md"], v.read("STYLE.md"));
    let (_, digest, _) = v.run_raw(&["digest"]);
    assert_eq!(env["data"]["digest"], digest);
    assert_json_snapshot!(v.redact(&env), {".loci" => "[version]"});
    // The human output has every part, in order.
    let (_, out, _) = v.run_raw(&["status"]);
    let order = ["## Plugins", "## Uncommitted changes", "## Inbox", "# Index", "# Rules", "# Style"];
    let pos: Vec<usize> = order.iter().map(|h| out.find(h).unwrap_or_else(|| panic!("no {h}:\n{out}"))).collect();
    assert!(pos.windows(2).all(|w| w[0] < w[1]), "{out}");
    // status writes nothing.
    assert_eq!(v.git(&["status", "--porcelain"]), "");
}

#[test]
fn status_reports_every_part_when_one_fails() {
    let v = TestVault::committed(BIN, "agent-workflow");
    std::fs::remove_file(v.root.join("STYLE.md")).unwrap();
    let (code, env) = v.run(&["status"]);
    assert_eq!(code, 0, "{env:#}");
    assert!(env["data"]["changes"].is_object());
    assert!(env["data"]["digest"].is_string());
    assert!(env["data"]["rules"]["RULES.md"].is_string());
    assert!(env["data"]["rules"]["STYLE.md"].is_null());
    assert!(has(&env, "status.rules", "STYLE.md"), "{env:#}");
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

fn dropped(env: &serde_json::Value) -> Vec<String> {
    with_code(env, "material-dropped").iter().map(|d| d["message"].as_str().unwrap().to_string()).collect()
}

#[test]
fn material_dropped_when_a_deleted_draft_is_not_absorbed() {
    let v = TestVault::committed(BIN, "agent-workflow");
    std::fs::remove_file(v.root.join("inbox/drafts/draft.md")).unwrap();
    let (code, env) = v.run(&["lint"]);
    assert_eq!(code, 0, "warnings only: {}", diag_lines(&env));
    let msgs = dropped(&env);
    assert_eq!(msgs.len(), 3, "{}", diag_lines(&env));
    assert!(with_code(&env, "material-dropped").iter().all(|d| d["severity"] == "warning"));
    assert_snapshot!(msgs.join("\n"));
}

#[test]
fn material_dropped_silent_when_absorbed() {
    let v = TestVault::committed(BIN, "agent-workflow");
    let draft = v.read("inbox/drafts/draft.md");
    std::fs::remove_file(v.root.join("inbox/drafts/draft.md")).unwrap();
    let body = draft.strip_prefix("# Draft\n").unwrap();
    v.write("concepts/a/x.md", v.read("concepts/a/x.md") + body);
    let (_, env) = v.run(&["lint"]);
    assert!(dropped(&env).is_empty(), "{}", diag_lines(&env));
    // Partly absorbed: only the code block is kept.
    let v = TestVault::committed(BIN, "agent-workflow");
    std::fs::remove_file(v.root.join("inbox/drafts/draft.md")).unwrap();
    v.write("views/kept.md", "# Kept\n\n```python\nprint(\"hi\")\n```\n");
    let (_, env) = v.run(&["lint"]);
    let msgs = dropped(&env);
    assert_eq!(msgs.len(), 2, "{}", diag_lines(&env));
    assert!(msgs.iter().all(|m| !m.contains("code")), "{msgs:?}");
}

#[test]
fn material_dropped_skipped_without_head() {
    let v = TestVault::git_no_head(BIN, "agent-workflow");
    std::fs::remove_file(v.root.join("inbox/drafts/draft.md")).unwrap();
    let (code, env) = v.run(&["lint"]);
    assert_eq!(code, 0, "{}", diag_lines(&env));
    assert!(dropped(&env).is_empty());
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
    assert!(is_ulid(id), "{id}");
    let text = v.read("concepts/a/sub/fresh.md");
    assert_eq!(text, format!("---\nid: {id}\nprereqs: []\ndigest: \"\"\n---\n# fresh\n"));
    assert_json_snapshot!(v.redact(&env), {".loci" => "[version]"});
    // The empty digest is fm.missing-field until filled.
    let (_, env) = v.run(&["lint"]);
    assert_eq!(codes_on(&env, "concepts/a/sub/fresh.md"), ["fm.missing-field"]);

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
    ] {
        let (exit, env) = v.run(&["new", path]);
        assert_eq!((exit, error_code(&env)), (1, code), "new {path}: {env:#}");
    }
    assert_eq!(v.tree(), before);
    v.write(".gitignore", "concepts/a/secret.md\n");
    v.commit_all("ignore");
    let (exit, env) = v.run(&["new", "concepts/a/secret"]);
    assert_eq!((exit, error_code(&env)), (1, "path.ignored"), "{env:#}");
}

#[test]
fn material_dropped_reports_on_the_deleted_path() {
    let v = TestVault::committed(BIN, "agent-workflow");
    std::fs::remove_file(v.root.join("inbox/drafts/draft.md")).unwrap();
    let (_, env) = v.run(&["lint"]);
    let d = with_code(&env, "material-dropped");
    assert!(!d.is_empty());
    assert!(d.iter().all(|d| d["path"] == "inbox/drafts/draft.md"), "{}", diag_lines(&env));
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
