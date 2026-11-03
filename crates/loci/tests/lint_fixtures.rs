//! `loci lint` (and the read-only `find`) on the fixture vaults of m0 §13.2.

mod common;

use common::*;
use insta::{assert_json_snapshot, assert_snapshot};

fn lint(name: &str) -> (TestVault, i32, serde_json::Value) {
    let v = TestVault::committed(BIN, name);
    let (code, env) = v.run(&["lint"]);
    assert_eq!(env["command"], "lint");
    (v, code, env)
}

/// Asserts that `path` has exactly the given set of diagnostic codes.
#[track_caller]
fn assert_codes(env: &serde_json::Value, path: &str, expected: &[&str]) {
    let mut exp: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    exp.sort();
    exp.dedup();
    assert_eq!(codes_on(env, path), exp, "diagnostics on {path}:\n{}", diag_lines(env));
}

/// The diagnostics on `path` with `code` and the source text each one covers.
fn covered_by(v: &TestVault, env: &serde_json::Value, code: &str, path: &str) -> Vec<String> {
    let text = v.read(path);
    with_code(env, code).into_iter().filter(|d| path_of(d) == path).map(|d| covered(&text, d).to_string()).collect()
}

// ---------------------------------------------------------------- names

#[test]
fn names() {
    let (v, code, env) = lint("names");
    assert_eq!(code, 1);
    let dfs = "concepts/algo/dfs.md";
    // `[[slug]]` resolves to a concept, written exactly (`[[BFS]]` does not); paths
    // resolve.
    let dangling = covered_by(&v, &env, "link.dangling", dfs);
    assert_eq!(dangling, ["BFS", "plan", "25-03-14", "diagram.png", "breadth-first"]);
    let hints: Vec<&str> = with_code(&env, "link.dangling").iter().map(|d| d["hint"].as_str().unwrap()).collect();
    assert_eq!(hints[0], "no concept is named `BFS`; did you mean [[bfs]]?");
    // Nothing close to suggest for the rest: create the concept, or link a non-concept by path.
    for d in &with_code(&env, "link.dangling")[1..] {
        let hint = d["hint"].as_str().unwrap();
        assert!(hint.starts_with("no concept is named") && hint.ends_with("or link a non-concept by path"), "{hint}");
    }
    assert_eq!(covered_by(&v, &env, "link.path-form", dfs), ["/RULES", "../x"]);
    // A view sharing a concept's stem is not a slug duplicate; two concepts are.
    let dups = with_code(&env, "slug.duplicate");
    assert_eq!(dups.len(), 1, "{}", diag_lines(&env));
    assert_eq!(path_of(dups[0]), "concepts/algo/dup.md");
    assert!(dups[0]["message"].as_str().unwrap().contains("concepts/other/dup.md"));
    assert!(on_path(&env, "views/learn/bfs.md").is_empty());
    assert_eq!(env["data"]["errors"], 8);
    assert_eq!(env["data"]["warnings"], 0);
    assert_json_snapshot!(v.redact(&env), {".loci" => "[version]"});
}

#[test]
fn lint_output_is_byte_identical_across_runs() {
    let v = TestVault::committed(BIN, "names");
    let a = v.run_raw(&["--json", "lint"]);
    let b = v.run_raw(&["--json", "lint"]);
    assert_eq!(a, b);
}

#[test]
fn diagnostics_are_sorted_by_path_offset_code() {
    for fixture in ["names", "frontmatter-edge", "sources", "code-hash", "cjk"] {
        let (_, _, env) = lint(fixture);
        let keys: Vec<(Vec<u8>, i64, String)> = diags(&env)
            .iter()
            .map(|d| {
                (
                    path_of(d).as_str().unwrap_or("").as_bytes().to_vec(),
                    span_of(d)["byte_start"].as_i64().unwrap_or(-1),
                    d["code"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "{fixture}:\n{}", diag_lines(&env));
    }
}

#[test]
fn positions_are_exact() {
    // line/col are 1-based, col counts scalar values, and byte offsets cover the target.
    let (v, _, env) = lint("cjk");
    let d = with_code(&env, "link.dangling-heading")[0];
    let text = v.read("views/linker.md");
    let start = span_of(d)["byte_start"].as_u64().unwrap() as usize;
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    assert_eq!(span_of(d)["line"], text[..start].matches('\n').count() + 1);
    assert_eq!(span_of(d)["col"], text[line_start..start].chars().count() + 1);
    // The target range runs from after `[[` to the first `|` or `]]` (§5.1).
    assert_eq!(covered(&text, d), "views/zh#不存在");
}

#[test]
fn messages_contain_no_positions() {
    let (_, _, env) = lint("frontmatter-edge");
    let re = regex::Regex::new(r"(?i)\b(line|column|byte)s?\s+\d").unwrap();
    for d in diags(&env) {
        let m = d["message"].as_str().unwrap();
        assert!(!re.is_match(m), "message with a position: {m}");
    }
}

// ---------------------------------------------------------------- link-syntax

#[test]
fn link_syntax() {
    let (v, code, env) = lint("link-syntax");
    assert_eq!(code, 1);
    let c = "concepts/a/c.md";
    assert_codes(&env, c, &["link.markdown"]);
    let md = with_code(&env, "link.markdown");
    let covered: Vec<_> = md.iter().map(|d| covered(&v.read(c), d).to_string()).collect();
    // Inline link, inline image and reference link; external links and autolinks pass.
    assert_eq!(covered.len(), 3, "{}", diag_lines(&env));
    assert!(covered[0].starts_with("[x](concepts/a/b.md)"), "{covered:?}");
    assert!(covered[1].starts_with("![](concepts/a/x.png)"), "{covered:?}");
    let hints: Vec<_> = md.iter().map(|d| d["hint"].as_str().unwrap().to_string()).collect();
    assert!(hints[0].contains("[[concepts/a/b]]"), "{hints:?}");
    assert!(hints[1].contains("![[concepts/a/x.png]]"), "{hints:?}");
    assert!(hints[2].contains("[[concepts/a/b]]"), "{hints:?}");
    // Wikilink embeds resolve by root path, also for two same-named images.
    assert!(with_code(&env, "link.dangling").is_empty());
    assert_snapshot!(diag_lines(&env));
}

// ---------------------------------------------------------------- dissolved-heading

#[test]
fn dissolved_heading() {
    let (v, code, env) = lint("dissolved-heading");
    assert_eq!(code, 1);
    let l = "concepts/a/linker.md";
    assert_codes(&env, l, &["link.dangling-heading", "link.dangling-block"]);
    assert_eq!(covered_by(&v, &env, "link.dangling-heading", l), ["target#Removed Heading", "#missing-local"]);
    let msgs: Vec<_> = with_code(&env, "link.dangling-heading").iter().map(|d| d["message"].clone()).collect();
    assert!(msgs[0].as_str().unwrap().contains("Removed Heading"));
    assert!(msgs[1].as_str().unwrap().contains("missing-local"));
    let block = with_code(&env, "link.dangling-block");
    assert_eq!(block.len(), 1);
    assert!(block[0]["message"].as_str().unwrap().contains("^gone"));
    assert_snapshot!(diag_lines(&env));
}

// ---------------------------------------------------------------- block-ids (lint)

#[test]
fn block_ids_resolve_and_duplicate() {
    let (v, _, env) = lint("block-ids");
    // Every `#^id` link of linker.md resolves: tight, loose, multi-line, parent, child,
    // paragraph, and the same id in two notes.
    assert_codes(&env, "concepts/a/linker.md", &[]);
    assert_codes(&env, "concepts/a/lists.md", &[]);
    let dup = with_code(&env, "block.id-duplicate");
    assert_eq!(dup.len(), 1, "{}", diag_lines(&env));
    assert_eq!(path_of(dup[0]), "concepts/a/dups.md");
    assert_eq!(covered(&v.read("concepts/a/dups.md"), dup[0]).trim(), "^same");
}

#[test]
fn block_id_of_parent_item_spans_its_children() {
    use loci_core::parse::{BlockKind, parse};
    use loci_core::vault::Kind;
    let text = std::fs::read_to_string(fixtures_dir().join("block-ids/concepts/a/lists.md")).unwrap();
    let note =
        parse(&loci_core::path::VaultPath::new("concepts/a/lists.md".into()).unwrap(), Kind::Concept, text.clone());
    let get = |id: &str| note.block_ids.iter().find(|b| b.id == id).unwrap_or_else(|| panic!("no ^{id}"));
    for id in ["tight1", "loose1", "multi1", "parent1", "child1", "para1", "shared"] {
        get(id);
    }
    let parent = get("parent1");
    let child = get("child1");
    // The parent's id sits before the nested list and its block covers the children.
    assert!(parent.range.end <= text.find("  - child item").unwrap());
    assert!(text[parent.block.clone()].contains("- child item ^child1"));
    assert!(text[parent.block.clone()].contains("- other child"));
    assert!(text[child.block.clone()].starts_with("child item") || text[child.block.clone()].contains("child item"));
    assert!(!text[child.block.clone()].contains("other child"));
    assert_eq!(parent.kind, child.kind);
    assert!(matches!(get("para1").kind, BlockKind::Paragraph));
    // The multi-line item's id identifies the whole item.
    assert!(text[get("multi1").block.clone()].contains("first line"));
}

// ---------------------------------------------------------------- no-h1

#[test]
fn no_h1() {
    let (_, code, env) = lint("no-h1");
    assert_eq!(code, 1);
    assert_codes(&env, "views/none.md", &["h1.missing"]);
    assert_codes(&env, "views/h2-only.md", &["h1.missing"]);
    assert_codes(&env, "views/late.md", &["h1.not-first"]);
    assert_codes(&env, "views/two.md", &["h1.multiple"]);
    assert_codes(&env, "views/setext-two.md", &["h1.multiple"]);
    assert_codes(&env, "views/setext.md", &[]);
    assert_codes(&env, "journal/r/2025/03/25-03-14.md", &["h1.missing"]);
    assert_codes(&env, "STYLE.md", &["h1.missing"]);
    assert_codes(&env, "concepts/a/fm-no-h1.md", &["h1.missing"]);
    assert_codes(&env, "concepts/a/fm-then-h1.md", &[]);
    // Not a managed kind.
    assert_codes(&env, "inbox/drafts/draft.md", &[]);
    assert_snapshot!(diag_lines(&env));
}

// ---------------------------------------------------------------- code-hash

#[test]
fn code_hash() {
    let (v, code, env) = lint("code-hash");
    assert_eq!(code, 1);
    // Only `[[dangling-x]]` in prose is a link: none in fences, indented code, code
    // spans or math; `$5 and ... $6` is text.
    assert_eq!(covered_by(&v, &env, "link.dangling", "concepts/a/code.md"), ["dangling-x"]);
    assert_codes(&env, "concepts/a/code.md", &["link.dangling"]);
    // `# not a heading` in a fence, `#tag` and `#notaheading` are not headings;
    // `## Real heading` is.
    let msgs: Vec<String> =
        with_code(&env, "link.dangling-heading").iter().map(|d| d["message"].as_str().unwrap().to_string()).collect();
    assert_eq!(msgs.len(), 3, "{}", diag_lines(&env));
    assert!(msgs[0].contains("not a heading") && msgs[1].contains("`tag`") && msgs[2].contains("notaheading"));
}

// ---------------------------------------------------------------- cjk

#[test]
fn cjk_lint() {
    let (v, _, env) = lint("cjk");
    // CJK slugs pass slug.format; CJK heading fragments and block ids resolve.
    assert!(with_code(&env, "slug.format").is_empty(), "{}", diag_lines(&env));
    assert_eq!(covered_by(&v, &env, "link.dangling-heading", "views/linker.md"), ["views/zh#不存在"]);
    assert_codes(&env, "views/linker.md", &["link.dangling-heading"]);
    // size.note counts characters, not bytes: 长文 is within the budget of 18, 更长 is
    // just over it.
    let size = with_code(&env, "size.note");
    assert_eq!(size.len(), 1, "{}", diag_lines(&env));
    assert_eq!(path_of(size[0]), "concepts/algo/更长.md");
    assert_eq!(size[0]["severity"], "warning");
    let msg = size[0]["message"].as_str().unwrap();
    let n: usize = regex::Regex::new(r"is (\d+) characters").unwrap().captures(msg).unwrap()[1].parse().unwrap();
    assert!((19..=20).contains(&n), "{msg}");
    assert!(msg.ends_with("(budget 18)"), "{msg}");
}

// ---------------------------------------------------------------- kinds

#[test]
fn kinds() {
    let v = TestVault::fixture(BIN, "kinds");
    // Untracked gitignored `.md` files are never read.
    v.write(".gitignore", "/secret/\n/.loci/\n");
    v.write("secret/s.md", "no heading [[ignored-dangling]] zanzibar\n");
    v.write(".loci/cache.md", "zanzibar\n");
    v.git(&["init", "-q", "-b", "main"]);
    v.commit_all("fixture");
    let (code, env) = v.run(&["lint"]);
    assert_eq!(code, 1);
    for p in ["notes.md", "concepts/x.md", "misc/readme.md"] {
        assert_codes(&env, p, &["kind.unknown"]);
    }
    assert_codes(&env, "inbox/drafts/d.md", &[]);
    assert_codes(&env, "inbox/loose.md", &["inbox.root-file"]);
    assert_codes(&env, "inbox/journal/entry.md", &["inbox.root-file"]);
    assert_codes(&env, "inbox/journal/r/entry.md", &[]);
    assert!(with_code(&env, "inbox.root-file").iter().all(|d| d["severity"] == "warning"));
    for d in diags(&env) {
        let p = path_of(d).as_str().unwrap_or("");
        assert!(!p.starts_with("plugins/") && !p.starts_with("secret/") && !p.starts_with(".loci/"), "{d}");
    }
    assert_snapshot!(diag_lines(&env));
}

// ---------------------------------------------------------------- table-pipe

#[test]
fn table_pipe_lint() {
    let (v, _, env) = lint("table-pipe");
    // `[[target\|label]]` and `[[views/v\|..]]` resolve; only `missing` dangles.
    let d = with_code(&env, "link.dangling");
    assert_eq!(d.len(), 1, "{}", diag_lines(&env));
    assert!(covered(&v.read("concepts/a/table.md"), d[0]).starts_with("missing"));
    assert!(with_code(&env, "link.dangling-heading").is_empty());
}

// ---------------------------------------------------------------- frontmatter-edge

#[test]
fn frontmatter_edge_lint() {
    let (v, code, env) = lint("frontmatter-edge");
    assert_eq!(code, 1);
    assert_codes(&env, "concepts/a/crlf.md", &[]);
    assert!(codes_on(&env, "concepts/a/unterminated.md").contains(&"fm.unterminated".to_string()));
    assert_codes(&env, "concepts/a/dupkeys.md", &["fm.yaml"]);
    assert_codes(&env, "concepts/a/scalars.md", &[]);
    assert_codes(&env, "concepts/a/unquoted.md", &["fm.field-type"]);
    let ft = with_code(&env, "fm.field-type")[0];
    assert_eq!(ft["hint"], "write \"[[slug]]\"");
    assert!(ft["message"].as_str().unwrap().contains("prereqs"));
    assert_codes(&env, "concepts/a/tags.md", &["fm.unknown-field"]);
    assert_eq!(covered_by(&v, &env, "fm.unknown-field", "concepts/a/tags.md"), ["tags"]);
    assert_codes(&env, "journal/r/2025/01/25-01-02.md", &["fm.not-allowed"]);
    assert_codes(&env, "journal/r/2025/01/25-01-01.md", &[]);
}

// ---------------------------------------------------------------- areas

#[test]
fn areas_lint() {
    let (_, code, env) = lint("areas");
    assert_eq!(code, 1);
    assert_codes(&env, "concepts/missing/AREA.md", &["area.missing"]);
    assert_codes(&env, "concepts/broken/AREA.md", &["area.format"]);
    let long = with_code(&env, "area.too-long");
    assert_eq!(long.len(), 1, "{}", diag_lines(&env));
    assert_eq!(path_of(long[0]), "concepts/algorithms/AREA.md");
    assert_eq!(long[0]["severity"], "warning");
    assert!(long[0]["message"].as_str().unwrap().contains("(budget 20)"));
    // Sub-folders at any depth hold concepts, without an AREA.md of their own.
    assert_codes(&env, "concepts/algorithms/graphs/deep/dijkstra.md", &[]);
}

// ---------------------------------------------------------------- sources (lint)

#[test]
fn sources_lint() {
    let (v, code, env) = lint("sources");
    assert_eq!(code, 1);
    // Citations are ordinary links: a page of a PDF and a heading of a markdown source
    // resolve; a missing heading or file does not.
    assert_eq!(covered_by(&v, &env, "link.dangling-heading", "concepts/a/c.md"), ["sources/docs/guide#nope"]);
    assert_eq!(covered_by(&v, &env, "link.dangling", "concepts/a/c.md"), ["sources/papers/missing.pdf"]);
    // Every source is `sources/<type>/<slug>.<ext>`, with any slug as the type.
    for bad in
        ["sources/stray.md", "sources/Talks/intro.md", "sources/clippings/nested/page.md", "sources/papers/noext"]
    {
        assert!(has(&env, "sources.layout", bad), "{bad}: {}", diag_lines(&env));
    }
    for good in ["sources/videos/talk.md", "sources/papers/heaps.pdf", "sources/docs/guide.md"] {
        assert!(on_path(&env, good).is_empty(), "{good}: {}", diag_lines(&env));
    }
    assert!(with_code(&env, "sources.changed").is_empty());
    assert_snapshot!(diag_lines(&env));
}

#[test]
fn sources_changed_after_edit() {
    let v = TestVault::committed(BIN, "sources");
    v.write("sources/papers/heaps.pdf", "%PDF-1.4\nedited\n");
    let (_, env) = v.run(&["lint"]);
    let changed = with_code(&env, "sources.changed");
    assert_eq!(changed.len(), 1, "{}", diag_lines(&env));
    assert_eq!(path_of(changed[0]), "sources/papers/heaps.pdf");
    // A stray file under `sources/` is not a source: only `sources.layout` applies.
    v.write("sources/stray.md", "# Stray, edited\n");
    let (_, env) = v.run(&["lint"]);
    assert!(!has(&env, "sources.changed", "sources/stray.md"), "{}", diag_lines(&env));
    v.git(&["checkout", "--", "sources/stray.md"]);
    // An edited source cannot be renamed, which would hide the edit.
    let (code, env) = v.run(&["mv", "sources/papers/heaps.pdf", "sources/papers/heap-notes.pdf"]);
    assert_eq!((code, error_code(&env)), (1, "mv.source-changed"), "{env:#}");
    // Renaming an unchanged source is not a change to it.
    v.git(&["checkout", "--", "sources/papers/heaps.pdf"]);
    v.run(&["mv", "sources/papers/heaps.pdf", "sources/papers/heap-notes.pdf"]);
    let (_, env) = v.run(&["lint"]);
    assert!(with_code(&env, "sources.changed").is_empty(), "{}", diag_lines(&env));
}

// ---------------------------------------------------------------- vault discovery

#[test]
fn vault_not_git_and_config_invalid() {
    let v = TestVault::fixture(BIN, "dispatch");
    let (code, env) = v.run(&["lint"]);
    assert_eq!((code, error_code(&env)), (1, "vault.not-git"));
    v.git(&["init", "-q", "-b", "main"]);
    v.write("loci.toml", "schema = 1\nunknown = 2\n");
    let (code, env) = v.run(&["lint"]);
    assert_eq!((code, error_code(&env)), (1, "config.invalid"));
    v.write("loci.toml", "schema = 2\n");
    let (code, env) = v.run(&["lint"]);
    assert_eq!((code, error_code(&env)), (1, "config.invalid"));
}

#[test]
fn vault_found_from_a_subdirectory_and_by_flag() {
    let v = TestVault::committed(BIN, "names");
    let (code, env) = v.run_in(&v.root.join("concepts/algo"), &["lint"]);
    assert_eq!(code, 1);
    assert_eq!(env["data"]["errors"], 8);
    let elsewhere = v.home();
    let (_, env) = v.run_in(&elsewhere, &["--vault", v.root.to_str().unwrap(), "lint"]);
    assert_eq!(env["data"]["errors"], 8);
    let (code, env) = v.run_in(&elsewhere, &["lint"]);
    assert_eq!((code, error_code(&env)), (1, "vault.not-found"));
}

#[test]
fn usage_error_exits_2_without_envelope() {
    let v = TestVault::committed(BIN, "names");
    let (code, out, _) = v.run_raw(&["--json", "mv", "only-one-arg"]);
    assert_eq!(code, 2);
    assert!(out.is_empty());
}

/// The vault is what git sees: tracked files, and untracked files that no ignore rule
/// (including the user's excludes file) matches.
#[test]
fn the_vault_is_what_git_sees() {
    let v = TestVault::committed(BIN, "mv-rename");
    // Tracked, then matched by `.gitignore`: git still tracks it, so loci reads it.
    v.write(".gitignore", "views/linker.md\n");
    v.commit_all("ignore a tracked file");
    let (_, env) = v.run(&["links", "views/linker.md"]);
    assert!(error_code(&env).is_empty(), "{env:#}");
    // Untracked and excluded by `core.excludesFile`: not in the vault, for lint and mv alike.
    v.write("excludes", "views/scratch.md\n");
    v.git(&["config", "core.excludesFile", v.root.join("excludes").to_str().unwrap()]);
    v.write("views/scratch.md", "[[nowhere]]\n");
    let (_, env) = v.run(&["lint"]);
    assert!(on_path(&env, "views/scratch.md").is_empty(), "{}", diag_lines(&env));
    let (code, env) = v.run(&["mv", "views/scratch.md", "views/kept.md"]);
    assert_eq!((code, error_code(&env)), (1, "mv.from"), "{env:#}");
}

/// loci never follows a symlink: a vault holding one, as a file or a folder, is refused
/// by every command, naming each link, until it is replaced.
#[test]
fn a_vault_with_a_symlink_is_refused() {
    let v = TestVault::committed(BIN, "mv-rename");
    std::fs::write(v.tmp.path().join("secret.md"), "secret-token-xyz\n").unwrap();
    std::os::unix::fs::symlink(v.tmp.path().join("secret.md"), v.root.join("views/leak.md")).unwrap();
    std::fs::create_dir(v.tmp.path().join("synced")).unwrap();
    std::os::unix::fs::symlink(v.tmp.path().join("synced"), v.root.join("views/synced")).unwrap();
    // Paths core never reads count too: no write ever goes through a link.
    std::fs::write(v.tmp.path().join("log.jsonl"), "").unwrap();
    std::fs::create_dir_all(v.root.join("plugins/learn")).unwrap();
    std::os::unix::fs::symlink(v.tmp.path().join("log.jsonl"), v.root.join("plugins/learn/reviews.jsonl")).unwrap();
    // A link with a non-UTF-8 name is refused too, for its name.
    let odd = v.root.join(<std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(b"views/odd\xff"));
    std::os::unix::fs::symlink(v.tmp.path().join("secret.md"), &odd).unwrap();
    for args in [&["lint"][..], &["status"], &["mv", "views/linker.md", "views/x.md"]] {
        let (code, env) = v.run(args);
        assert_eq!((code, error_code(&env)), (1, "file.symlink"), "{args:?}: {env:#}");
        let message = env["error"]["message"].as_str().unwrap();
        for link in ["`views/leak.md`", "`views/synced`", "`plugins/learn/reviews.jsonl`"] {
            assert!(message.lines().any(|l| l.trim_start().starts_with(link)), "{link}: {message}");
        }
    }
    std::fs::remove_file(v.root.join("views/leak.md")).unwrap();
    std::fs::remove_file(v.root.join("views/synced")).unwrap();
    std::fs::remove_file(v.root.join("plugins/learn/reviews.jsonl")).unwrap();
    let (code, env) = v.run(&["lint"]);
    assert_eq!((code, error_code(&env)), (1, "file.unreadable"), "{env:#}");
    assert!(env["error"]["message"].as_str().unwrap().contains("has a name that is not UTF-8"), "{env:#}");
    std::fs::remove_file(&odd).unwrap();
    let (code, env) = v.run(&["lint"]);
    assert!(error_code(&env).is_empty() && code <= 1, "{env:#}");
}

/// `material-dropped` counts only vault files: `plugins/` is never read (m0 §3).
#[test]
fn material_dropped_ignores_plugin_data() {
    let v = TestVault::committed(BIN, "mv-rename");
    v.write("plugins/x/README.md", "# X\n\n```\ncode\n```\n");
    v.commit_all("plugin data");
    std::fs::remove_file(v.root.join("plugins/x/README.md")).unwrap();
    let (_, env) = v.run(&["lint"]);
    assert!(with_code(&env, "material-dropped").is_empty(), "{}", diag_lines(&env));
}

/// Every name git lists must be UTF-8, even where core does not read (`plugins/`): a
/// vault holding one that is not is refused (`file.unreadable`), naming it.
#[test]
fn a_non_utf8_name_anywhere_refuses_the_vault() {
    use std::os::unix::ffi::OsStrExt;
    let v = TestVault::committed(BIN, "mv-rename");
    let raw = |p: &[u8]| v.root.join(std::ffi::OsStr::from_bytes(p));
    std::fs::create_dir_all(v.root.join("plugins/x")).unwrap();
    std::fs::write(raw(b"plugins/x/data\xff"), "x").unwrap();
    v.commit_all("odd");
    for args in [&["lint"][..], &["status"]] {
        let (code, env) = v.run(args);
        assert_eq!((code, error_code(&env)), (1, "file.unreadable"), "{args:?}: {env:#}");
        let message = env["error"]["message"].as_str().unwrap();
        assert!(message.contains("`plugins/x/data\u{fffd}` has a name that is not UTF-8"), "{message}");
    }
    std::fs::remove_file(raw(b"plugins/x/data\xff")).unwrap();
    let (code, env) = v.run(&["lint"]);
    assert_eq!((code, error_code(&env)), (1, "file.unreadable"), "a tracked name gone from disk: {env:#}");
    v.commit_all("odd removed");
    let (code, env) = v.run(&["lint"]);
    assert!(error_code(&env).is_empty() && code <= 1, "{env:#}");
}

/// `init` refuses a symlink among the files it manages before writing anything.
#[test]
fn init_respects_the_vault_boundary() {
    let fresh = TestVault::empty(BIN);
    std::os::unix::fs::symlink(fresh.tmp.path().join("made.md"), fresh.root.join("AGENTS.md")).unwrap();
    let names = || {
        let mut n: Vec<_> = std::fs::read_dir(&fresh.root).unwrap().map(|e| e.unwrap().file_name()).collect();
        n.sort();
        n
    };
    let before = names();
    let (code, env) = fresh.run(&["init"]);
    assert_eq!((code, error_code(&env)), (1, "file.symlink"), "{env:#}");
    assert_eq!(names(), before, "nothing written");
    assert!(!fresh.tmp.path().join("made.md").exists());
}

/// A tracked file that is gone from disk is a pending deletion, not an unreadable file,
/// even when its folder became a file; but a tracked name that is not UTF-8 refuses the
/// vault until its deletion is committed, as every name git lists must be UTF-8.
#[test]
fn a_deleted_file_does_not_refuse_the_vault() {
    use std::os::unix::ffi::OsStrExt;
    let v = TestVault::committed(BIN, "mv-rename");
    v.write("views/a/b.md", "# B\n");
    std::fs::create_dir_all(v.root.join("views/sub")).unwrap();
    std::fs::write(v.root.join(std::ffi::OsStr::from_bytes(b"views/sub/odd\xff.png")), "x").unwrap();
    v.write("views/sub/gone.png", "x");
    v.write("views/sub/s.md", "# S\n");
    v.commit_all("files to delete");
    std::fs::remove_dir_all(v.root.join("views/a")).unwrap();
    v.write("views/a", "# A\n");
    std::fs::remove_file(v.root.join(std::ffi::OsStr::from_bytes(b"views/sub/odd\xff.png"))).unwrap();
    let (code, env) = v.run(&["status"]);
    assert_eq!((code, error_code(&env)), (1, "file.unreadable"), "{env:#}");
    assert!(env["error"]["message"].as_str().unwrap().contains("has a name that is not UTF-8"), "{env:#}");
    v.git(&["add", "-u", "views/sub"]);
    v.git(&["commit", "-q", "-m", "odd deleted"]);
    std::fs::remove_file(v.root.join("views/sub/gone.png")).unwrap();
    let (code, env) = v.run(&["status"]);
    assert_eq!(code, 0, "{env:#}");
    let (code, env) = v.run(&["mv", "views/sub", "views/other"]);
    assert_eq!(code, 0, "{env:#}");
    assert!(v.exists("views/other/s.md"));
}

/// Every file core reads must be readable: one that is not, by content, permission,
/// folder or name, refuses the vault for every command, naming each.
#[test]
fn an_unreadable_file_refuses_the_vault() {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;
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
        let message = env["error"]["message"].as_str().unwrap();
        for part in [
            "`views/bad.md` is not valid UTF-8",
            "`views/locked.md` cannot be read",
            "`sources/papers/p.pdf` cannot be read",
            "`views/closed/inner.md` cannot be read",
            "`views/odd\u{fffd}.png` has a name that is not UTF-8",
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

/// A tracked file replaced on disk: by a folder, it is gone (the folder's files are
/// listed on their own); by a socket or FIFO, which opening could block on, the vault is
/// refused (`file.unreadable`).
#[test]
fn a_tracked_file_replaced_by_a_folder_or_a_special_file() {
    let v = TestVault::with_files(BIN, &[("views/a.md", "# A\n"), ("views/b.md", "# B\n"), ("views/c.md", "# C\n")]);
    std::fs::remove_file(v.root.join("views/a.md")).unwrap();
    v.write("views/a.md/inner.md", "# Inner\n");
    let (code, env) = v.run(&["lint"]);
    assert!(error_code(&env).is_empty() && code == 0, "a folder is not refused: {env:#}");
    std::fs::remove_file(v.root.join("views/b.md")).unwrap();
    let _socket = std::os::unix::net::UnixListener::bind(v.root.join("views/b.md")).unwrap();
    let (code, env) = v.run(&["lint"]);
    assert_eq!((code, error_code(&env)), (1, "file.unreadable"), "{env:#}");
    assert!(env["error"]["message"].as_str().unwrap().contains("`views/b.md` is not a regular file"), "{env:#}");
}
