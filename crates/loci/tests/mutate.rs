//! Mutating commands on the fixture vaults (m0 §13.2): `mv`, `block-ids`, `init`. Goldens hold the envelope and the resulting tree with ULIDs redacted.

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
fn mv_rename_rewrites_links() {
    let v = TestVault::committed(BIN, "mv-rename");
    let before = v.read("concepts/a/renamed.md");
    let env = mv(&v, "concepts/a/renamed.md", "concepts/a/renamed-new.md");
    assert_json_snapshot!(v.redact(&env), {".loci" => "[version]"});
    // The note itself is unchanged; links by bare slug stay bare; markdown links are
    // never edited.
    assert_eq!(v.read("concepts/a/renamed-new.md"), before);
    let linker = v.read("views/linker.md");
    assert!(linker.contains("- [[renamed-new]] and [[renamed-new|label]] and [[renamed-new]]\n"), "{linker}");
    assert!(linker.contains("[markdown](concepts/a/renamed.md)"), "{linker}");
    assert!(v.read("concepts/a/prereq-holder.md").contains(r#"prereqs: ["[[renamed-new]]", "[[heading-target]]"]"#));
    assert!(!v.exists("concepts/a/renamed.md"));
    assert_snapshot!(v.tree());
}

#[test]
fn mv_rename_rewrites_path_links() {
    let v = TestVault::committed(BIN, "mv-rename");
    mv(&v, "concepts/a/block-target.md", "concepts/a/block-target-new.md");
    let linker = v.read("views/linker.md");
    assert!(linker.contains("- [[concepts/a/block-target-new]] and [[concepts/a/block-target-new#^b]]\n"), "{linker}");
}

#[test]
fn mv_rename_rewrites_fragments_and_prereqs() {
    let v = TestVault::committed(BIN, "mv-rename");
    mv(&v, "concepts/a/heading-target.md", "concepts/a/heading-target-new.md");
    assert!(v.read("views/linker.md").contains("- [[heading-target-new#Heading]]\n"));
    assert!(v.read("concepts/a/prereq-holder.md").contains(r#"prereqs: ["[[renamed]]", "[[heading-target-new]]"]"#));
    // Renaming back is a rename like any other.
    mv(&v, "concepts/a/heading-target-new.md", "concepts/a/heading-target.md");
    assert!(v.read("concepts/a/prereq-holder.md").contains(r#"prereqs: ["[[renamed]]", "[[heading-target]]"]"#));
    // The vault still lints the same: no new link errors.
    let (_, env) = v.run(&["lint"]);
    assert_eq!(env["data"]["errors"], 1, "{}", diag_lines(&env)); // the markdown link
}

#[test]
fn mv_fixes_a_non_nfc_name() {
    let v = TestVault::committed(BIN, "mv-rename");
    v.write("views/cafe\u{301}.md", "# Cafe\n");
    v.commit_all("nfd");
    let (_, env) = v.run(&["lint"]);
    assert!(has(&env, "path.not-nfc", "views/cafe\u{301}.md"), "{}", diag_lines(&env));
    // Named by its NFC form, as typed.
    mv(&v, "views/caf\u{e9}.md", "views/cafe.md");
    let (_, env) = v.run(&["lint"]);
    assert!(with_code(&env, "path.not-nfc").is_empty(), "{}", diag_lines(&env));
}

#[test]
fn a_folder_name_is_never_a_pattern() {
    let v = TestVault::committed(BIN, "mv-rename");
    v.write("views/q*/a.md", "# A\n");
    v.write("views/qa/b.md", "# B\n");
    v.commit_all("look-alike folders");
    mv(&v, "views/q*", "views/z");
    assert!(v.exists("views/z/a.md") && v.exists("views/qa/b.md"));
}

#[test]
fn mv_refusals() {
    let v = TestVault::committed(BIN, "mv-rename");
    mv_refused(&v, "concepts/a/renamed.md", "concepts/a/block-target.md", "mv.exists");
    mv_refused(&v, "concepts/a/renamed.md", "concepts/a/sub/block-target.md", "mv.collision");
    mv_refused(&v, "concepts/a/renamed.md", "concepts/a/sub/Block-target.md", "slug.format");
    mv_refused(&v, "concepts/a/renamed.md", "concepts/a/Upper.md", "slug.format");
    mv_refused(&v, "sources", "old/sources", "mv.source-file");
    mv_refused(&v, "concepts/a/renamed.md", "views/renamed.md", "mv.no-area");
    mv_refused(&v, "concepts/a/nope.md", "concepts/a/x.md", "mv.from");
    mv_refused(&v, "concepts/a/renamed.md", "../out.md", "path.outside");
    mv_refused(&v, "plugins/learn/state.json", "views/state.json", "mv.from");
    mv_refused(&v, "views/linker.md", "plugins/learn/linker.md", "mv.to");
    mv_refused(&v, "sources/papers/p1.pdf", "sources/papers/p1/x.pdf", "mv.source-file");
    mv_refused(&v, "sources/papers/p1.pdf", "views/p1.pdf", "mv.source-file");
    mv_refused(&v, "views/linker.md", "sources/papers/Linker.md", "sources.layout");
}

#[test]
fn mv_ignored_paths_are_refused() {
    let v = TestVault::fixture(BIN, "mv-rename");
    v.write(".gitignore", "/.loci/\n/ignored/\n");
    v.write("ignored/x.md", "# X\n");
    v.git(&["init", "-q", "-b", "main"]);
    v.commit_all("fixture");
    mv_refused(&v, "ignored/x.md", "views/x.md", "mv.from");
    mv_refused(&v, "views/linker.md", ".loci/linker.md", "path.ignored");
}

#[test]
fn mv_source_renames_citations() {
    let v = TestVault::committed(BIN, "mv-rename");
    mv(&v, "sources/papers/p1.pdf", "sources/papers/p2.pdf");
    assert!(v.exists("sources/papers/p2.pdf") && !v.exists("sources/papers/p1.pdf"));
    assert!(v.read("concepts/a/renamed.md").contains("[[sources/papers/p2.pdf#p2]]"));
    assert!(v.read("views/linker.md").contains("![[sources/papers/p2.pdf]]"));
    let (_, env) = v.run(&["lint"]);
    assert!(with_code(&env, "link.dangling").is_empty(), "{}", diag_lines(&env));
}

#[test]
fn mv_directory() {
    let v = TestVault::committed(BIN, "mv-rename");
    let env = mv(&v, "views/sub", "views/other");
    assert_eq!(env["data"]["moved"], serde_json::json!([["views/sub/inner.md", "views/other/inner.md"]]));
    assert!(v.read("views/linker.md").contains("- [[views/other/inner]]\n"));
    assert!(v.read("views/other/inner.md").contains("Back to [[views/linker]]."));
    assert!(!v.exists("views/sub"));
}

#[test]
fn mv_area_directory_renames_nothing_but_paths() {
    let v = TestVault::committed(BIN, "mv-rename");
    mv(&v, "concepts/a", "concepts/b");
    let linker = v.read("views/linker.md");
    assert!(linker.contains("[[concepts/b/block-target]]") && linker.contains("- [[renamed]] and"), "{linker}");
    let (_, env) = v.run(&["lint"]);
    assert_eq!(env["data"]["errors"], 1, "{}", diag_lines(&env));
}

// ---------------------------------------------------------------- table-pipe

#[test]
fn table_pipe_mv_keeps_escaped_pipe() {
    let v = TestVault::committed(BIN, "table-pipe");
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
fn frontmatter_edge_crlf_note_moves_unchanged() {
    let v = TestVault::committed(BIN, "frontmatter-edge");
    mv(&v, "concepts/a/crlf.md", "concepts/a/crlf-two.md");
    assert_eq!(
        v.read("concepts/a/crlf-two.md"),
        "---\r\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0J1\r\n---\r\n# CRLF\r\n\r\nBody line.\r\n"
    );
}

#[test]
fn frontmatter_edge_prereqs_edited_inside_quotes() {
    let v = TestVault::committed(BIN, "frontmatter-edge");
    mv(&v, "journal/r/2025/01/25-01-01.md", "journal/r/2025/01/25-01-03.md");
    mv(&v, "concepts/a/target.md", "concepts/a/target-two.md");
    let text = v.read("concepts/a/scalars.md");
    assert_eq!(
        text,
        "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0J6\nprereqs: [\"[[target-two]]\"]\n---\n\
         # Scalars\n\nSee [[journal/r/2025/01/25-01-03]] and [[journal/r/2025/01/25-01-03#Morning]].\n"
    );
    let (_, env) = v.run(&["lint"]);
    assert!(on_path(&env, "concepts/a/scalars.md").is_empty(), "{}", diag_lines(&env));
}

// ---------------------------------------------------------------- cjk

#[test]
fn cjk_mv_edits_frontmatter_at_the_right_offset() {
    let v = TestVault::committed(BIN, "cjk");
    let env = mv(&v, "concepts/algo/排序.md", "concepts/algo/排序算法.md");
    assert_json_snapshot!(v.redact(&env), {".loci" => "[version]"});
    assert_eq!(
        v.read("concepts/algo/二分查找.md"),
        "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0G1\nprereqs: [\"[[排序算法]]\"]\n---\n# 二分\n"
    );
    assert_eq!(v.read("concepts/algo/排序算法.md"), "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0G2\n---\n# 排序\n");
    mv(&v, "views/zh.md", "views/中文.md");
    assert_eq!(
        v.read("views/linker.md"),
        "# Linker\n\n[[views/中文#复杂度分析]] [[views/中文#^要点一]] [[二分查找]] [[views/中文#不存在]]\n"
    );
}

// ---------------------------------------------------------------- block-ids

fn block_ids(v: &TestVault, note: &str, section: &str) -> (i32, serde_json::Value) {
    let (code, env) = v.run(&["block-ids", note, "--section", section]);
    assert_eq!(env["command"], "block-ids");
    (code, env)
}

#[test]
fn block_ids_assigns_top_level_blocks_of_the_section() {
    let v = TestVault::committed(BIN, "block-ids");
    let note = "concepts/a/cards.md";
    let before = v.read(note);
    let (code, env) = block_ids(&v, note, "Cards");
    assert_eq!(code, 0, "{env:#}");
    let after = v.read(note);
    // Only insertions of ` ^<ULID>`, all ids distinct ULIDs.
    assert_eq!(strip_block_ids(&after), before);
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
    let v = TestVault::committed(BIN, "block-ids");
    let (code, env) = block_ids(&v, "views/v.md", "Cards");
    assert_eq!(code, 0, "{env:#}");
    assert_eq!(env["data"]["assigned"].as_array().unwrap().len(), 1);
    assert!(redact_ulids(&v.read("views/v.md")).contains("- a view item ^[ulid]\n"));

    let before = v.tree();
    let (code, env) = block_ids(&v, "sources/docs/guide.md", "Cards");
    assert_eq!((code, error_code(&env)), (1, "block-ids.kind"));
    let (code, env) = block_ids(&v, "concepts/a/cards.md", "Nope");
    assert_eq!((code, error_code(&env)), (1, "block-ids.no-section"));
    // Like every note argument, `.md` is optional and a concept slug works.
    for arg in ["concepts/a/cards", "cards"] {
        let (code, env) = block_ids(&v, arg, "Nope");
        assert_eq!((code, error_code(&env)), (1, "block-ids.no-section"), "{arg}: {env:#}");
    }
    // `### Deeper heading` is not an H2.
    let (code, env) = block_ids(&v, "concepts/a/cards.md", "Deeper heading");
    assert_eq!((code, error_code(&env)), (1, "block-ids.no-section"));
    assert_eq!(v.tree(), before);
}

// ---------------------------------------------------------------- filing sources

#[test]
fn an_inbox_file_is_filed_as_a_source_with_mv() {
    let v = TestVault::committed(BIN, "sources");
    let clip = v.read("inbox/drafts/clip.md");
    mv(&v, "inbox/drafts/clip.md", "sources/clippings/clip.md");
    assert_eq!(v.read("sources/clippings/clip.md"), clip);
    mv(&v, "inbox/drafts/paper.pdf", "sources/papers/inbox-paper.pdf");
    let (_, env) = v.run(&["lint"]);
    for p in ["sources/clippings/clip.md", "sources/papers/inbox-paper.pdf"] {
        assert!(on_path(&env, p).is_empty(), "{p}: {}", diag_lines(&env));
    }
    // The source layout holds: a type and a slug, both slugs, and an extension.
    v.write("inbox/drafts/again.md", "# Again\n");
    v.commit_all("more");
    for to in ["sources/Clips/again.md", "sources/docs/Bad_Slug.md", "sources/again.md"] {
        mv_refused(&v, "inbox/drafts/again.md", to, "sources.layout");
    }
    mv_refused(&v, "inbox/drafts/again.md", "sources/docs/again", "mv.to");
}

// ---------------------------------------------------------------- init

fn init_in(dir: &std::path::Path, v: &TestVault) -> (i32, serde_json::Value) {
    let (code, env) = v.run_in(dir, &["init"]);
    assert_eq!(env["command"], "init");
    (code, env)
}

#[test]
fn init_creates_a_vault_without_committing() {
    let v = TestVault::empty(BIN);
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
    let v = TestVault::fixture(BIN, "init");
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
    assert_eq!(v.read(".gitignore"), "target/\n/.loci/\n", "an existing line is not added again");
}

#[test]
fn init_keeps_a_non_utf8_gitignore() {
    let v = TestVault::empty(BIN);
    let original = b"# caf\xe9 build outputs\nsecrets.env\n";
    std::fs::write(v.root.join(".gitignore"), original).unwrap();
    let (code, env) = init_in(&v.root, &v);
    assert_eq!(code, 0, "{env:#}");
    let mut expected = original.to_vec();
    expected.extend_from_slice(b"/.loci/\n");
    assert_eq!(std::fs::read(v.root.join(".gitignore")).unwrap(), expected);
}

#[test]
fn init_inside_a_work_tree_but_not_at_its_root_is_refused() {
    let v = TestVault::empty(BIN);
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
    let v = TestVault::committed(BIN, "mv-rename");
    std::fs::write(v.tmp.path().join("outside.md"), "# Outside\n").unwrap();
    let before = v.tree();
    for args in [
        &["new", "concepts/a/../../../escaped"][..],
        &["block-ids", "views/../../outside.md", "--section", "X"],
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
fn mv_never_overwrites_reserved_or_unindexed_files() {
    let v = TestVault::committed(BIN, "mv-rename");
    v.write("views/x.png", "garbage");
    v.write(".gitignore", "*.log\n");
    v.write("views/old.log", "ignored but real");
    v.commit_all("more");
    for to in ["loci.toml", ".gitignore", ".git/config", "views/old.log"] {
        let before = std::fs::read(v.root.join(to)).unwrap();
        let (exit, env) = v.run(&["mv", "views/x.png", to]);
        assert_eq!(exit, 1, "{to}: {env:#}");
        assert!(["mv.to", "mv.exists", "path.ignored"].contains(&error_code(&env)), "{to}: {env:#}");
        assert_eq!(std::fs::read(v.root.join(to)).unwrap(), before, "{to} was overwritten");
    }
}
