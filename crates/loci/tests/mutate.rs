//! Mutating commands on the fixture vaults (m0 §13.2): `mv`, `block-ids`, `fetch`,
//! `init`. Goldens hold the envelope and the resulting tree with ULIDs redacted.

mod common;

use common::*;
use insta::{assert_json_snapshot, assert_snapshot};

fn mv(v: &TestVault, from: &str, to: &str) -> serde_json::Value {
    let (code, env) = v.run(&["mv", from, to]);
    assert_eq!(code, 0, "mv {from} {to}: {env:#}");
    assert_eq!(env["command"], "mv");
    env
}

/// Asserts that `mv from to` is refused with `code` and writes nothing.
#[track_caller]
fn mv_refused(v: &TestVault, from: &str, to: &str, code: &str) {
    let before = v.tree();
    let (exit, env) = v.run(&["mv", from, to]);
    assert_eq!((exit, error_code(&env)), (1, code), "mv {from} {to}: {env:#}");
    assert_eq!(v.tree(), before, "mv {from} {to} wrote something");
}

// ---------------------------------------------------------------- mv-rename

#[test]
fn mv_rename_flow_aliases() {
    let v = TestVault::committed("mv-rename");
    let env = mv(&v, "concepts/a/flow.md", "concepts/a/flow-new.md");
    assert_json_snapshot!(v.redact(&env), {".loci" => "[version]"});
    // Old slug appended to a flow list; links by bare slug stay bare; markdown links
    // are never edited.
    assert!(v.read("concepts/a/flow-new.md").contains("aliases: [f1, flow]\n"));
    let linker = v.read("views/linker.md");
    assert!(linker.contains("- [[flow-new]] and [[flow-new|label]] and [[flow-new]]\n"), "{linker}");
    assert!(linker.contains("[markdown](concepts/a/flow.md)"), "{linker}");
    assert!(v.read("concepts/a/empty-flow.md").contains(r#"prereqs: ["[[flow-new]]", "[[absent]]"]"#));
    assert!(!v.exists("concepts/a/flow.md"));
    assert_snapshot!(v.tree());
}

#[test]
fn mv_rename_block_aliases() {
    let v = TestVault::committed("mv-rename");
    let env = mv(&v, "concepts/a/block.md", "concepts/a/block-new.md");
    assert_eq!(
        env["data"]["aliases_added"],
        serde_json::json!([{"path": "concepts/a/block-new.md", "alias": "block"}])
    );
    assert!(v.read("concepts/a/block-new.md").contains("aliases:\n  - b1\n  - block\ndigest:"));
    let linker = v.read("views/linker.md");
    assert!(linker.contains("- [[concepts/a/block-new]] and [[concepts/a/block-new#^b]]\n"), "{linker}");
}

#[test]
fn mv_rename_absent_and_empty_aliases() {
    let v = TestVault::committed("mv-rename");
    mv(&v, "concepts/a/absent.md", "concepts/a/absent-new.md");
    assert!(
        v.read("concepts/a/absent-new.md")
            .starts_with("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0M3\naliases: [absent]\ndigest:")
    );
    assert!(v.read("views/linker.md").contains("- [[absent-new#Heading]]\n"));
    assert!(v.read("concepts/a/empty-flow.md").contains(r#"prereqs: ["[[flow]]", "[[absent-new]]"]"#));
    mv(&v, "concepts/a/empty-flow.md", "concepts/a/ef.md");
    assert!(v.read("concepts/a/ef.md").contains("aliases: [empty-flow]\n"));
    // The vault still lints the same: no new link errors.
    let (_, env) = v.run(&["lint"]);
    assert_eq!(env["data"]["errors"], 1, "{}", diag_lines(&env)); // the markdown link
}

#[test]
fn mv_refusals() {
    let v = TestVault::committed("mv-rename");
    mv_refused(&v, "concepts/a/flow.md", "concepts/a/block.md", "mv.exists");
    mv_refused(&v, "concepts/a/flow.md", "concepts/a/sub/block.md", "mv.collision");
    mv_refused(&v, "concepts/a/flow.md", "concepts/a/b1.md", "mv.collision");
    mv_refused(&v, "concepts/a/flow.md", "concepts/a/B1.md", "mv.collision");
    mv_refused(&v, "concepts/a/flow.md", "views/flow.md", "mv.no-area");
    mv_refused(&v, "concepts/a/nope.md", "concepts/a/x.md", "mv.from");
    mv_refused(&v, "concepts/a/flow.md", "../out.md", "path.outside");
    mv_refused(&v, "plugins/learn/state.json", "views/state.json", "mv.from");
    mv_refused(&v, "views/linker.md", "plugins/learn/linker.md", "mv.to");
    mv_refused(&v, "sources/papers/p1/p1.pdf", "sources/papers/p1/x.pdf", "mv.source-file");
    mv_refused(&v, "sources/papers/p1/p1.pdf", "views/p1.pdf", "mv.source-file");
    mv_refused(&v, "views/linker.md", "sources/papers/p1/linker.md", "mv.source-file");
}

#[test]
fn mv_ignored_paths_are_refused() {
    let v = TestVault::fixture("mv-rename");
    v.write(".gitignore", "/.loci/\n/ignored/\n");
    v.write("ignored/x.md", "# X\n");
    v.git(&["init", "-q", "-b", "main"]);
    v.commit_all("fixture");
    mv_refused(&v, "ignored/x.md", "views/x.md", "mv.from");
    mv_refused(&v, "views/linker.md", ".loci/linker.md", "mv.to");
}

#[test]
fn mv_source_folder_moves_as_a_unit() {
    let v = TestVault::committed("mv-rename");
    let env = mv(&v, "sources/papers/p1", "sources/papers/p2");
    let moved = env["data"]["moved"].as_array().unwrap();
    assert_eq!(moved.len(), 2);
    assert!(v.exists("sources/papers/p2/p1.pdf") && v.exists("sources/papers/p2/source.yaml"));
    assert!(!v.exists("sources/papers/p1"));
    assert!(v.read("concepts/a/flow.md").contains(r#"sources: ["sources/papers/p2#p2"]"#));
    assert!(v.read("views/linker.md").contains("![[sources/papers/p2/p1.pdf]]"));
    let (_, env) = v.run(&["lint"]);
    assert!(with_code(&env, "sources.unresolved").is_empty(), "{}", diag_lines(&env));
}

#[test]
fn mv_directory() {
    let v = TestVault::committed("mv-rename");
    let env = mv(&v, "views/sub", "views/other");
    assert_eq!(env["data"]["moved"], serde_json::json!([["views/sub/inner.md", "views/other/inner.md"]]));
    assert!(v.read("views/linker.md").contains("- [[views/other/inner]]\n"));
    assert!(v.read("views/other/inner.md").contains("Back to [[views/linker]]."));
    assert!(!v.exists("views/sub"));
}

#[test]
fn mv_area_directory_renames_nothing_but_paths() {
    let v = TestVault::committed("mv-rename");
    let env = mv(&v, "concepts/a", "concepts/b");
    assert_eq!(env["data"]["aliases_added"], serde_json::json!([]));
    let linker = v.read("views/linker.md");
    assert!(linker.contains("[[concepts/b/block]]") && linker.contains("- [[flow]] and"), "{linker}");
    let (_, env) = v.run(&["lint"]);
    assert_eq!(env["data"]["errors"], 1, "{}", diag_lines(&env));
}

// ---------------------------------------------------------------- table-pipe

#[test]
fn table_pipe_mv_keeps_escaped_pipe() {
    let v = TestVault::committed("table-pipe");
    mv(&v, "concepts/a/target.md", "concepts/a/renamed.md");
    mv(&v, "views/v.md", "views/w.md");
    let t = v.read("concepts/a/table.md");
    assert!(t.contains("| a | [[renamed\\|Target label]] |\n"), "{t}");
    assert!(t.contains("| b | [[renamed#Part\\|part]] |\n"), "{t}");
    assert!(t.contains("| c | [[views/w\\|the view]] |\n"), "{t}");
    assert!(t.contains("| d | [[missing\\|nothing]] |\n"), "{t}");
    assert_snapshot!(t);
}

// ---------------------------------------------------------------- frontmatter-edge

#[test]
fn frontmatter_edge_crlf_alias_line() {
    let v = TestVault::committed("frontmatter-edge");
    mv(&v, "concepts/a/crlf.md", "concepts/a/crlf-two.md");
    assert_eq!(
        v.read("concepts/a/crlf-two.md"),
        "---\r\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0J1\r\naliases: [crlf]\r\ndigest: A CRLF note.\r\n---\r\n# CRLF\r\n\r\nBody line.\r\n"
    );
}

#[test]
fn frontmatter_edge_scalars_edited_inside_quotes() {
    let v = TestVault::committed("frontmatter-edge");
    mv(&v, "journal/r/2025/01/25-01-01.md", "journal/r/2025/01/25-01-03.md");
    mv(&v, "concepts/a/target.md", "concepts/a/target-two.md");
    let text = v.read("concepts/a/scalars.md");
    assert_eq!(
        text,
        "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0J6\nprereqs: [\"[[target-two]]\"]\nsources:\n  \
         - \"journal/r/2025/01/25-01-03\"\n  - 'journal/r/2025/01/25-01-03#Morning'\n  \
         - journal/r/2025/01/25-01-03\ndigest: Quoted and plain scalars.\n---\n# Scalars\n"
    );
    let (_, env) = v.run(&["lint"]);
    assert!(on_path(&env, "concepts/a/scalars.md").is_empty(), "{}", diag_lines(&env));
}

// ---------------------------------------------------------------- cjk

#[test]
fn cjk_mv_edits_frontmatter_at_the_right_offset() {
    let v = TestVault::committed("cjk");
    let env = mv(&v, "concepts/algo/排序.md", "concepts/algo/排序算法.md");
    assert_json_snapshot!(v.redact(&env), {".loci" => "[version]"});
    assert_eq!(
        v.read("concepts/algo/二分查找.md"),
        "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0G1\nprereqs: [\"[[排序算法]]\"]\ndigest: 在有序数组中查找\n---\n# 二分\n"
    );
    assert_eq!(
        v.read("concepts/algo/排序算法.md"),
        "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0G2\naliases: [排序]\ndigest: 排列元素\n---\n# 排序\n"
    );
    mv(&v, "views/zh.md", "views/中文.md");
    assert_eq!(
        v.read("views/linker.md"),
        "# Linker\n\n[[views/中文#复杂度分析]] [[views/中文#^要点一]] [[二分查找]] [[views/zh#不存在]]\n"
    );
}

// ---------------------------------------------------------------- block-ids

fn block_ids(v: &TestVault, note: &str, section: &str) -> (i32, serde_json::Value) {
    let (code, env) = v.run(&["block-ids", note, "--section", section]);
    assert_eq!(env["command"], "block-ids");
    (code, env)
}

/// Removes every ` ^<ULID>` the command inserted.
fn strip_ulid_ids(s: &str) -> String {
    regex::Regex::new(r" \^[0-9A-HJKMNP-TV-Z]{26}").unwrap().replace_all(s, "").into_owned()
}

#[test]
fn block_ids_assigns_top_level_blocks_of_the_section() {
    let v = TestVault::committed("block-ids");
    let note = "concepts/a/cards.md";
    let before = v.read(note);
    let (code, env) = block_ids(&v, note, "Cards");
    assert_eq!(code, 0, "{env:#}");
    let after = v.read(note);
    // Only insertions of ` ^<ULID>`, all ids distinct ULIDs.
    assert_eq!(strip_ulid_ids(&after), before);
    let assigned = env["data"]["assigned"].as_array().unwrap();
    let ids: Vec<&str> = assigned.iter().map(|a| a["id"].as_str().unwrap()).collect();
    assert!(ids.iter().all(|i| is_ulid(i)), "{ids:?}");
    let mut uniq = ids.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(uniq.len(), ids.len());
    let lines: Vec<u64> = assigned.iter().map(|a| a["line"].as_u64().unwrap()).collect();
    for (id, line) in ids.iter().zip(&lines) {
        let l = after.lines().nth(*line as usize - 1).unwrap();
        assert!(l.ends_with(&format!(" ^{id}")), "line {line}: {l}");
    }
    assert_eq!(env["data"]["path"], note);
    assert_eq!(env["data"]["section"], "Cards");
    assert_snapshot!(redact_ulids(&after));
    // Idempotent.
    let (code, env) = block_ids(&v, note, "Cards");
    assert_eq!(code, 0);
    assert_eq!(env["data"]["assigned"], serde_json::json!([]));
    assert_eq!(v.read(note), after);
    // The new ids are ordinary block ids.
    let (_, env) = v.run(&["lint"]);
    assert!(on_path(&env, note).is_empty(), "{}", diag_lines(&env));
}

#[test]
fn block_ids_any_managed_note_and_refusals() {
    let v = TestVault::committed("block-ids");
    let (code, env) = block_ids(&v, "views/v.md", "Cards");
    assert_eq!(code, 0, "{env:#}");
    assert_eq!(env["data"]["assigned"].as_array().unwrap().len(), 1);
    assert!(redact_ulids(&v.read("views/v.md")).contains("- a view item ^[ulid]\n"));

    let before = v.tree();
    let (code, env) = block_ids(&v, "sources/docs/guide/guide.md", "Cards");
    assert_eq!((code, error_code(&env)), (1, "block-ids.kind"));
    let (code, env) = block_ids(&v, "concepts/a/cards.md", "Nope");
    assert_eq!((code, error_code(&env)), (1, "block-ids.no-section"));
    // `### Deeper heading` is not an H2.
    let (code, env) = block_ids(&v, "concepts/a/cards.md", "Deeper heading");
    assert_eq!((code, error_code(&env)), (1, "block-ids.no-section"));
    assert_eq!(v.tree(), before);
}

// ---------------------------------------------------------------- fetch

#[test]
fn fetch_from_inbox() {
    let v = TestVault::committed("sources");
    let clip = v.read("inbox/drafts/clip.md");
    let (code, env) = v.run(&["fetch", "inbox/drafts/clip.md", "clippings/clip"]);
    assert_eq!(code, 0, "{env:#}");
    assert_eq!(env["command"], "fetch");
    let sha = env["data"]["sha256"].as_str().unwrap().to_string();
    assert_eq!(sha.len(), 64);
    assert_eq!(
        env["data"],
        serde_json::json!({"path": "sources/clippings/clip", "file": "clip.md", "sha256": sha, "url": null})
    );
    assert!(!v.exists("inbox/drafts/clip.md"));
    assert_eq!(v.read("sources/clippings/clip/clip.md"), clip);
    assert_eq!(v.read("sources/clippings/clip/source.yaml"), format!("sha256: {sha}\n"));

    let (code, env) = v.run(&["fetch", "inbox/drafts/paper.pdf", "papers/inbox-paper"]);
    assert_eq!(code, 0, "{env:#}");
    assert_eq!(env["data"]["file"], "paper.pdf");
    assert!(v.exists("sources/papers/inbox-paper/paper.pdf"));
    assert!(!v.read("sources/papers/inbox-paper/source.yaml").contains("url:"));

    // The new folders are clean.
    let (_, env) = v.run(&["lint"]);
    for d in diags(&env) {
        let p = d["path"].as_str().unwrap_or("");
        assert!(!p.starts_with("sources/clippings/clip/") && !p.starts_with("sources/papers/inbox-paper/"), "{d}");
    }
    let mut tree = String::new();
    for p in v
        .paths()
        .iter()
        .filter(|p| p.starts_with("sources/clippings/clip/") || p.starts_with("sources/papers/inbox-paper/"))
    {
        tree.push_str(&format!("== {p} ==\n{}\n", String::from_utf8_lossy(&std::fs::read(v.root.join(p)).unwrap())));
    }
    assert_snapshot!(tree);
}

#[test]
fn fetch_refusals() {
    let v = TestVault::committed("sources");
    v.write("inbox/drafts/again.md", "# Again\n");
    let before = v.tree();
    let (code, env) = v.run(&["fetch", "inbox/drafts/again.md", "papers/heaps"]);
    assert_eq!((code, error_code(&env)), (1, "fetch.dest"));
    let (code, env) = v.run(&["fetch", "inbox/drafts/again.md", "videos/again"]);
    assert_eq!((code, error_code(&env)), (1, "fetch.dest"));
    let (code, env) = v.run(&["fetch", "inbox/drafts/again.md", "docs/Bad_Slug"]);
    assert_eq!((code, error_code(&env)), (1, "fetch.dest"));
    assert_eq!(v.tree(), before);
}

// ---------------------------------------------------------------- init

fn init_in(dir: &std::path::Path, v: &TestVault) -> (i32, serde_json::Value) {
    let (code, env) = v.run_in(dir, &["init"]);
    assert_eq!(env["command"], "init");
    (code, env)
}

#[test]
fn init_creates_a_vault_without_committing() {
    let v = TestVault::empty();
    let (code, env) = init_in(&v.root, &v);
    assert_eq!(code, 0, "{env:#}");
    assert!(v.root.join(".git").is_dir());
    let mut c = std::process::Command::new(v.bin().join("git"));
    let head =
        c.arg("-C").arg(&v.root).args(["rev-parse", "--verify", "-q", "HEAD"]).env("HOME", v.home()).output().unwrap();
    assert!(!head.status.success(), "init committed");
    assert!(!v.exists("inbox") && !v.exists(".gitattributes") && !v.exists("plugins"));
    assert_json_snapshot!(v.redact(&env), {".loci" => "[version]"});
    assert_snapshot!(v.tree());
    // The created vault opens and lints clean.
    let (code, env) = v.run(&["lint"]);
    assert_eq!(code, 0, "{}", diag_lines(&env));
    // init.exists.
    let before = v.tree();
    let (code, env) = init_in(&v.root, &v);
    assert_eq!((code, error_code(&env)), (1, "init.exists"));
    assert_eq!(v.tree(), before);
}

#[test]
fn init_skips_existing_files() {
    let v = TestVault::fixture("init");
    let (code, env) = init_in(&v.root, &v);
    assert_eq!((code, error_code(&env)), (1, "init.exists"));
    std::fs::remove_file(v.root.join("loci.toml")).unwrap();
    let (code, env) = init_in(&v.root, &v);
    assert_eq!(code, 0, "{env:#}");
    let skipped: Vec<&str> = env["data"]["skipped"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
    assert!(skipped.contains(&"RULES.md"), "{env:#}");
    let created: Vec<&str> = env["data"]["created"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
    for f in ["loci.toml", "STYLE.md", "AGENTS.md"] {
        assert!(created.contains(&f), "{env:#}");
    }
    assert_eq!(v.read("RULES.md"), "# My rules\n");
    // Missing .gitignore lines are appended once; existing ones are kept.
    assert_eq!(v.read(".gitignore"), "target/\n/.agents/\n/.claude/skills\n/.loci/\n");
}

#[test]
fn init_keeps_a_non_utf8_gitignore() {
    let v = TestVault::empty();
    let original = b"# caf\xe9 build outputs\nsecrets.env\n/.loci/\n";
    std::fs::write(v.root.join(".gitignore"), original).unwrap();
    let (code, env) = init_in(&v.root, &v);
    assert_eq!(code, 0, "{env:#}");
    let mut expected = original.to_vec();
    expected.extend_from_slice(b"/.agents/\n/.claude/skills\n");
    assert_eq!(std::fs::read(v.root.join(".gitignore")).unwrap(), expected);
}

#[test]
fn init_inside_a_work_tree_but_not_at_its_root_is_refused() {
    let v = TestVault::empty();
    v.git(&["init", "-q", "-b", "main"]);
    let sub = v.root.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let (code, env) = init_in(&sub, &v);
    assert_eq!((code, error_code(&env)), (1, "vault.not-git"));
    assert!(!sub.join("loci.toml").exists());
}

// ---------------------------------------------------------------- path arguments

#[test]
fn path_arguments_outside_the_vault_are_refused() {
    let v = TestVault::committed("mv-rename");
    std::fs::write(v.tmp.path().join("outside.md"), "# Outside\n").unwrap();
    let before = v.tree();
    for args in [
        &["new", "concepts/a/../../../escaped"][..],
        &["block-ids", "views/../../outside.md", "--section", "X"],
        &["fetch", "inbox/../../outside.md", "papers/p"],
        &["export", "/etc/passwd"],
        &["merge", "concepts/./a/flow.md", "concepts/a/flow.md"],
        &["mv", "concepts//a/flow.md", "concepts/a/x.md"],
    ] {
        let (exit, env) = v.run(args);
        assert_eq!((exit, error_code(&env)), (1, "path.outside"), "{args:?}: {env:#}");
    }
    assert_eq!(v.tree(), before);
    assert!(!v.tmp.path().join("escaped.md").exists());
    assert_eq!(std::fs::read_to_string(v.tmp.path().join("outside.md")).unwrap(), "# Outside\n");
}

#[test]
fn mv_rename_needs_valid_frontmatter() {
    let v = TestVault::committed("mv-rename");
    v.write("concepts/a/bare.md", "# Bare\n");
    v.write("concepts/a/scalar.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0Z1\naliases: one\ndigest: d\n---\n# S\n");
    v.commit_all("more");
    mv_refused(&v, "concepts/a/bare.md", "concepts/a/bare2.md", "mv.frontmatter");
    mv_refused(&v, "concepts/a/scalar.md", "concepts/a/scalar2.md", "mv.frontmatter");
}

#[test]
fn mv_never_overwrites_reserved_or_unindexed_files() {
    let v = TestVault::committed("mv-rename");
    v.write("views/x.png", "garbage");
    v.write(".gitignore", "*.log\n");
    v.write("views/old.log", "ignored but real");
    v.commit_all("more");
    for to in ["loci.toml", ".gitignore", ".git/config", "views/old.log"] {
        let before = std::fs::read(v.root.join(to)).unwrap();
        let (exit, env) = v.run(&["mv", "views/x.png", to]);
        assert_eq!(exit, 1, "{to}: {env:#}");
        assert!(["mv.to", "mv.exists"].contains(&error_code(&env)), "{to}: {env:#}");
        assert_eq!(std::fs::read(v.root.join(to)).unwrap(), before, "{to} was overwritten");
    }
}

#[test]
fn fetch_refuses_a_file_named_source_yaml() {
    let v = TestVault::committed("sources");
    v.write("inbox/drafts/source.yaml", "IMPORTANT USER DATA");
    let before = v.tree();
    let (exit, env) = v.run(&["fetch", "inbox/drafts/source.yaml", "docs/x"]);
    assert_eq!((exit, error_code(&env)), (1, "fetch.from"), "{env:#}");
    assert_eq!(v.tree(), before);
    assert_eq!(v.read("inbox/drafts/source.yaml"), "IMPORTANT USER DATA");
}
