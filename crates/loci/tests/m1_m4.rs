//! `merge`, `closure`, `bench`, `export` and `links` (docs/specs/m1-m4.md).

mod common;

use common::*;
use serde_json::json;

fn vault(files: &[(&str, String)]) -> TestVault {
    TestVault::with_files(BIN, files)
}

fn strs(v: &serde_json::Value) -> Vec<&str> {
    v.as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect()
}

#[test]
fn merge_rewrites_links_and_deletes_the_absorbed_note() {
    let v = vault(&[
        ("concepts/g/bfs.md", concept(IDS[0], "", "# BFS\n\n## Queue\n\nText.\n")),
        ("concepts/g/graph-search.md", concept(IDS[1], "prereqs: [\"[[bfs]]\"]\n", "# Graph search\n\nSee [[bfs]].\n")),
        (
            "concepts/g/dijkstra.md",
            concept(
                IDS[2],
                "prereqs: [\"[[bfs]]\"]\n",
                "# Dijkstra\n\n[[concepts/g/bfs#Queue|the queue]] and [[bfs]].\n",
            ),
        ),
    ]);
    // A dry run reports the same plan and writes nothing.
    let before = v.tree();
    let (code, dry) = v.run(&["merge", "bfs", "graph-search", "--dry-run"]);
    assert_eq!((code, &dry["data"]["dry_run"]), (0, &serde_json::json!(true)), "{dry}");
    assert_eq!(v.tree(), before);
    let (code, env) = v.run(&["merge", "bfs", "graph-search"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(dry["data"]["edited"], env["data"]["edited"]);
    assert!(!v.exists("concepts/g/bfs.md"));
    let into = v.read("concepts/g/graph-search.md");
    assert!(!into.contains("prereqs") && !into.contains("\"[[graph-search]]\""), "self-prereq removed:\n{into}");
    assert!(!into.contains("aliases"), "no aliases are added:\n{into}");
    assert!(into.contains("See [[graph-search]]."));
    let d = v.read("concepts/g/dijkstra.md");
    assert!(d.contains("prereqs:\n  - \"[[graph-search]]\"\n"), "{d}");
    assert!(
        d.contains("[[concepts/g/graph-search#Queue|the queue]] and [[graph-search]]."),
        "form, fragment and label kept:\n{d}"
    );
    assert_eq!(env["data"]["from"], "concepts/g/bfs.md");
    assert_eq!(env["data"]["into"], "concepts/g/graph-search.md");
    // The fragment into the deleted note now dangles; lint reports it.
    let (_, env) = v.run(&["lint"]);
    assert!(has(&env, "link.dangling-heading", "concepts/g/dijkstra.md"), "{env}");
}

#[test]
fn merge_refusals_write_nothing() {
    let v = vault(&[("concepts/g/bfs.md", concept(IDS[0], "", "# BFS\n")), ("views/plan.md", "# Plan\n".into())]);
    let before = v.tree();
    let (code, env) = v.run(&["merge", "bfs", "bfs"]);
    assert_eq!((code, error_code(&env)), (1, "merge.same"));
    let (code, env) = v.run(&["merge", "views/plan.md", "bfs"]);
    assert_eq!((code, error_code(&env)), (1, "note.kind"));
    assert_eq!(v.tree(), before);
}

#[test]
fn closure_orders_prerequisites_first() {
    let p = |xs: &[&str]| {
        format!("prereqs: [{}]\n", xs.iter().map(|x| format!("\"[[{x}]]\"")).collect::<Vec<_>>().join(", "))
    };
    let v = vault(&[
        ("concepts/g/top.md", concept(IDS[0], &p(&["mid", "base", "nope"]), "# Top\n")),
        ("concepts/g/mid.md", concept(IDS[1], &p(&["base"]), "# Mid\n")),
        ("concepts/g/base.md", concept(IDS[2], "", "# Base\n")),
    ]);
    let (code, env) = v.run(&["closure", "top"]);
    assert_eq!(code, 0, "{env}");
    let c = &env["data"]["closure"];
    let order: Vec<_> =
        c.as_array().unwrap().iter().map(|m| (m["slug"].as_str().unwrap(), m["depth"].as_u64().unwrap())).collect();
    assert_eq!(order, [("base", 1), ("mid", 1)]);
    assert_eq!(c[0]["path"], "concepts/g/base.md");
    assert_eq!(env["data"]["unresolved"], json!([{"from": "concepts/g/top.md", "target": "nope"}]));
    let (_, human, _) = v.run_raw(&["closure", "top"]);
    assert_eq!(human, "- [[base]]\n- [[mid]]\n");

    let (code, env) = v.run(&["closure", "missing"]);
    assert_eq!((code, error_code(&env)), (1, "note.unknown"));
    // A concept path is accepted in the same forms as elsewhere (`./`, `.md`).
    let (code, env) = v.run(&["closure", "./concepts/g/top.md"]);
    assert_eq!(code, 0, "{env:#}");
    let (code, env) = v.run(&["closure", "../top"]);
    assert_eq!((code, error_code(&env)), (1, "path.outside"));
}

/// A concept whose prereqs cannot all be read (an item that is no `"[[slug]]"`, or
/// broken frontmatter) gets a warning, so the closure is not taken as complete.
#[test]
fn closure_warns_of_unread_prereqs() {
    let v = vault(&[
        ("concepts/g/top.md", concept(IDS[0], "prereqs: [\"[[mid]]\", \"[[odd]]\"]\n", "# Top\n")),
        ("concepts/g/mid.md", concept(IDS[1], "prereqs: [[base]]\n", "# Mid\n")),
        ("concepts/g/odd.md", concept(IDS[2], "id: again\n", "# Odd\n")),
        ("concepts/g/base.md", concept(IDS[3], "", "# Base\n")),
    ]);
    let (code, env) = v.run(&["closure", "top"]);
    assert_eq!(code, 0, "{env:#}");
    let unread: Vec<_> = with_code(&env, "prereqs.unread").iter().map(|d| path_of(d).clone()).collect();
    assert_eq!(unread, [json!("concepts/g/mid.md"), json!("concepts/g/odd.md")], "{env:#}");
}

#[test]
fn closure_depth_and_cycles() {
    let p = |x: &str| format!("prereqs: [\"[[{x}]]\"]\n");
    let v = vault(&[
        ("concepts/g/a.md", concept(IDS[0], &p("b"), "# A\n")),
        ("concepts/g/b.md", concept(IDS[1], &p("c"), "# B\n")),
        ("concepts/g/c.md", concept(IDS[2], &p("b"), "# C\n")),
    ]);
    let (code, env) = v.run(&["closure", "a", "--depth", "1"]);
    assert_eq!(code, 0, "{env}");
    let slugs: Vec<_> =
        env["data"]["closure"].as_array().unwrap().iter().map(|m| m["slug"].as_str().unwrap()).collect();
    assert_eq!(slugs, ["b"]);
    let (code, env) = v.run(&["closure", "a"]);
    assert_eq!(code, 0, "a cycle is a warning here: {env}");
    let cyc = with_code(&env, "prereqs.cycle");
    assert_eq!(cyc.len(), 1, "{env}");
    assert_eq!(cyc[0]["severity"], "warning");
    let slugs: Vec<_> =
        env["data"]["closure"].as_array().unwrap().iter().map(|m| m["slug"].as_str().unwrap()).collect();
    assert_eq!(slugs, ["b", "c"], "the cycle is broken by slug bytes");
    // Members at the depth limit still order by their prerequisites and close cycles.
    let (_, env) = v.run(&["closure", "a", "--depth", "2"]);
    assert_eq!(with_code(&env, "prereqs.cycle").len(), 1, "{env}");
    let v = vault(&[
        ("concepts/g/r.md", concept(IDS[0], "prereqs: [\"[[a]]\", \"[[b]]\"]\n", "# R\n")),
        ("concepts/g/a.md", concept(IDS[1], &p("b"), "# A\n")),
        ("concepts/g/b.md", concept(IDS[2], "", "# B\n")),
    ]);
    let (_, env) = v.run(&["closure", "r", "--depth", "1"]);
    let slugs: Vec<_> =
        env["data"]["closure"].as_array().unwrap().iter().map(|m| m["slug"].as_str().unwrap()).collect();
    assert_eq!(slugs, ["b", "a"], "{env}");
}

#[test]
fn bench_runs_the_agent_on_every_case() {
    let cases = concat!(
        r#"{"q": "queue level", "check": "grep -q '\\[\\[bfs\\]\\]' \"$LOCI_BENCH_OUTPUT\""}"#,
        "\n\n",
        r#"{"q": "isolation", "check": "test -f views/new.md && ! grep -q 'LEAK\\|SEEN' \"$LOCI_BENCH_OUTPUT\""}"#,
        "\n",
        r#"{"q": "fails", "check": "exit 1"}"#,
        "\n",
    );
    let v = vault(&[
        ("concepts/g/bfs.md", concept(IDS[0], "", "# BFS\n\nVisits by level with a queue.\n")),
        ("views/plan.md", "# Plan\n\nNothing relevant.\n".into()),
        ("bench/cases.jsonl", cases.into()),
    ]);
    // Reports whether it sees the cases or an earlier case's edit, answers, and edits.
    let agent = "test -d bench && echo LEAK; test -f views/new.md && echo SEEN; echo '[[bfs]]'; : > views/new.md";
    // The test PATH holds only `git`: the agent cannot start.
    let (code, env) = v.run(&["bench", agent]);
    assert_eq!((code, error_code(&env)), (3, "bench.agent"));
    for tool in ["sh", "grep", "head"] {
        std::os::unix::fs::symlink(find_on_path(tool), v.bin().join(tool)).unwrap();
    }
    let (code, env) = v.run(&["bench", agent]);
    assert_eq!(code, 0, "{env}");
    let d = &env["data"];
    assert_eq!(d["cases"], 3, "{env}");
    let passes: Vec<bool> = d["results"].as_array().unwrap().iter().map(|r| r["pass"].as_bool().unwrap()).collect();
    // The reply is checked; each case runs on a fresh copy without `bench/`, where the
    // agent's edit is seen by the check; a failing check fails the case.
    assert_eq!(passes, [true, true, false], "{env:#}");
    assert_eq!(d["results"][0]["agent_exit"], 0, "{env:#}");
    let (_, env) = v.run(&["bench", "exit 5"]);
    assert_eq!(env["data"]["results"][0]["agent_exit"], 5, "a failing agent shows: {env:#}");
    assert!(!v.exists("views/new.md"), "the vault itself is untouched");

    // Cases may live outside the vault.
    let outside = v.tmp.path().join("cases.jsonl");
    std::fs::write(&outside, cases).unwrap();
    let (code, env) = v.run(&["bench", agent, "--cases", outside.to_str().unwrap()]);
    assert_eq!((code, env["data"]["cases"].clone()), (0, json!(3)), "{env:#}");

    // Output larger than a pipe buffer does not block the agent.
    let start = std::time::Instant::now();
    let (code, env) = v.run(&["bench", "head -c 200000 /dev/zero; echo '[[bfs]]'"]);
    assert_eq!(code, 0, "{env:#}");
    assert_eq!(env["data"]["results"][0]["pass"], true);
    assert!(start.elapsed() < std::time::Duration::from_secs(30));

    v.write("bench/cases.jsonl", "{\"q\": \"x\", \"check\": \"true\"}\n{\"q\": \"x\", \"expect\": []}\n");
    let (code, env) = v.run(&["bench", "true"]);
    assert_eq!((code, error_code(&env)), (1, "bench.case"));
    assert!(error_message(&env).contains("line 2"), "{env}");

    v.write("bench/cases.jsonl", "\n");
    let (code, env) = v.run(&["bench", "true"]);
    assert_eq!((code, error_code(&env)), (1, "bench.none"), "no cases");
    std::fs::remove_file(v.root.join("bench/cases.jsonl")).unwrap();
    let (code, env) = v.run(&["bench", "true"]);
    assert_eq!((code, error_code(&env)), (1, "bench.none"));
}

#[test]
fn bench_gives_the_copy_a_repository_of_its_own() {
    let v = vault(&[("views/plan.md", "# Plan\n".into())]);
    for tool in ["sh", "grep"] {
        std::os::unix::fs::symlink(find_on_path(tool), v.bin().join(tool)).unwrap();
    }
    // The copy has the vault's HEAD and index; the agent commits there.
    let cases = r#"{"q": "x", "check": "grep -q staged.md staged.txt && git log -1 --format=%s | grep -q agent"}"#;
    let agent =
        "git diff --cached --name-only > staged.txt && git rm -q views/plan.md && git add -A && git commit -qm agent";
    let bench = |root: &std::path::Path, git_dir: Option<&std::path::Path>| {
        std::fs::write(root.join("bench/cases.jsonl"), cases).unwrap();
        std::fs::write(root.join("views/staged.md"), "# Staged\n").unwrap();
        v.git(&["-C", root.to_str().unwrap(), "add", "views/staged.md"]);
        let head = v.git(&["-C", root.to_str().unwrap(), "rev-parse", "HEAD"]);
        let mut c = v.command();
        c.current_dir(root).args(["--json", "bench", agent]);
        if let Some(git_dir) = git_dir {
            c.env("GIT_DIR", git_dir);
        }
        let (code, env) = v.run_command(c, "");
        assert_eq!((code, &env["data"]["results"][0]["pass"]), (0, &json!(true)), "{env:#}");
        assert_eq!(
            v.git(&["-C", root.to_str().unwrap(), "rev-parse", "HEAD"]),
            head,
            "the vault's branch is untouched"
        );
        let status = v.git(&["-C", root.to_str().unwrap(), "status", "--porcelain", "--", "views"]);
        assert_eq!(status, "A  views/staged.md\n", "the vault's index and files are untouched");
    };
    std::fs::create_dir(v.root.join("bench")).unwrap();

    // A linked worktree's `.git` is a file naming the repository it shares; its split
    // index keeps a shared part beside it.
    let wt = v.tmp.path().join("wt");
    v.git(&["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "wt"]);
    v.git(&["-C", wt.to_str().unwrap(), "update-index", "--split-index"]);
    // git's fsmonitor daemon listens on a socket in each work tree's git folder.
    let socket = |dir: &str| std::os::unix::net::UnixListener::bind(v.root.join(dir).join("fsmonitor--daemon.ipc"));
    let _sockets = (socket(".git").unwrap(), socket(".git/worktrees/wt").unwrap());
    std::fs::create_dir(wt.join("bench")).unwrap();
    bench(&wt, None);

    // A repository may name its work tree in its config (as a submodule's does), and
    // loci may run with `GIT_DIR` set (from a git hook).
    v.git(&["config", "core.worktree", v.root.to_str().unwrap()]);
    bench(&v.root, Some(&v.root.join(".git")));
    assert_eq!(v.git(&["config", "core.worktree"]).trim(), v.root.to_str().unwrap(), "the vault's config is untouched");
}

#[test]
fn export_collects_links_and_assets() {
    let v = vault(&[
        (
            "views/study.md",
            "# Study\n\nStart with [[bfs]], then [[dijkstra#Relaxation]].\n\n![[assets/g/grid.png]]\n".into(),
        ),
        ("concepts/g/bfs.md", concept(IDS[0], "", "# BFS\n\n## Queue\n\nUses a queue; see [[graph]].\n")),
        (
            "concepts/g/dijkstra.md",
            concept(IDS[1], "", "# Dijkstra\n\nIntro.\n\n## Relaxation\n\nRelax edges.\n\n## Other\n\nNot exported.\n"),
        ),
        ("concepts/g/graph.md", concept(IDS[2], "", "# Graph\n\nBody.\n")),
        ("assets/g/grid.png", "PNG".into()),
    ]);
    let (code, env) = v.run(&["export", "views/study"]);
    assert_eq!(code, 0, "{env}");
    let d = &env["data"];
    assert_eq!(d["output"].as_str().unwrap(), v.root.join(".loci/export/views/study.md/study.md").to_str().unwrap());
    assert_eq!(strs(&d["notes"]), ["views/study.md", "concepts/g/bfs.md", "concepts/g/dijkstra.md"]);
    assert_eq!(strs(&d["assets"]), ["study.assets/grid.png"]);
    assert_eq!(v.read(".loci/export/views/study.md/study.assets/grid.png"), "PNG");

    // Depth 2 pulls graph in.
    let (_, env) = v.run(&["export", "views/study", "--depth", "2"]);
    assert!(strs(&env["data"]["notes"]).contains(&"concepts/g/graph.md"));

    // The export dir is ignored by lint (.loci/ is gitignored by init).
    v.write(".gitignore", ".loci/\n");
    let (_, env) = v.run(&["lint"]);
    assert!(with_code(&env, "kind.unknown").is_empty(), "{env}");
}

#[test]
fn export_refusals() {
    let v = vault(&[("areas/g/AREA.md", "# G\n".into())]);
    let (code, env) = v.run(&["export", "nothing"]);
    assert_eq!((code, error_code(&env)), (1, "note.unknown"));
    let (code, env) = v.run(&["export", "areas/g/AREA.md"]);
    assert_eq!((code, error_code(&env)), (1, "note.kind"));
    // An io error names the path that failed.
    v.write("blocker", "a file, not a folder\n");
    v.write("views/v.md", "# V\n");
    let (code, env) = v.run(&["export", "views/v", "-o", "blocker/out.md"]);
    assert_eq!((code, error_code(&env)), (3, "io"));
    assert!(env["error"]["message"].as_str().unwrap().contains("blocker"), "{env:#}");
}

/// HTML is rendered by loci itself and needs no network: every `#anchor` link points at
/// a heading `id` in the page, math is MathML, and `-o` is relative to the working
/// directory.
#[test]
fn export_html_is_rendered_in_process() {
    let v = vault(&[
        ("views/plan.md", "# Plan\n\n## Proof\n\nSee [[bfs]].\n\n> ## Quoted\n".into()),
        (
            "concepts/g/bfs.md",
            concept(IDS[0], "", "# BFS\n\nCost $O(V+E)$, and $a_1 < b_2$.\n\n## Proof\n\nBy induction.\n"),
        ),
    ]);
    for out in ["out/plan.html", "plan.html"] {
        let (code, env) = v.run(&["export", "views/plan", "--depth", "1", "--to", "html", "-o", out]);
        assert_eq!(code, 0, "{out}: {env:#}");
        let page = v.read(out);
        assert!(page.starts_with("<!doctype html>") && page.contains("<title>Plan</title>"), "{page}");
        let ids: Vec<&str> = page.split("id=\"").skip(1).map(|s| s.split('"').next().unwrap()).collect();
        let links: Vec<&str> = page.split("href=\"#").skip(1).map(|s| s.split('"').next().unwrap()).collect();
        // A heading in a block quote is no heading of the note (m0 §4.6): no identifier.
        assert_eq!(ids, ["plan", "proof", "bfs", "proof-1"], "{page}");
        assert!(page.contains("<h2>Quoted</h2>"), "{page}");
        assert!(!links.is_empty() && links.iter().all(|l| ids.contains(l)), "{links:?} in {ids:?}");
        assert_eq!(page.matches("<math").count(), 2, "{page}");
        assert!(page.contains("<msub>") && page.contains("O(V+E)"), "math is MathML, source kept: {page}");
        assert!(!page.contains("http"), "nothing from the network: {page}");
    }
    assert!(!v.exists("plan.assets") && !v.exists("out/plan.assets"), "no images, no images folder");
    let (code, env) = v.run(&["export", "views/plan", "-o", "/"]);
    assert_eq!((code, error_code(&env)), (3, "io"), "a path that names no file: {env:#}");
}

/// Each default export has its own folder, so one export never overwrites another's
/// images.
#[test]
fn default_exports_keep_their_own_images() {
    let v = vault(&[
        ("views/p/p.md", "# P\n\n![[views/p/g.png]]\n".to_string()),
        ("views/p/g.png", "AAA".to_string()),
        ("views/q/p.md", "# Q\n\n![[views/q/g.png]]\n".to_string()),
        ("views/q/g.png", "BBB".to_string()),
        // Its folder would sit inside `views/p/p.md`'s `p.assets/` if `.md` were cut.
        ("views/p/p/p.assets/g.png.md", "# R\n\n![[views/q/g.png]]\n".to_string()),
    ]);
    // The same stem, in different folders.
    for view in ["views/p/p", "views/q/p", "views/p/p/p.assets/g.png.md"] {
        let (code, env) = v.run(&["export", view]);
        assert_eq!(code, 0, "{env:#}");
    }
    assert_eq!(v.read(".loci/export/views/p/p.md/p.assets/g.png"), "AAA");
    assert_eq!(v.read(".loci/export/views/q/p.md/p.assets/g.png"), "BBB");
}

/// Exports written with `-o` into one folder keep their own images: each output's
/// images go in a folder named by the output's stem.
#[test]
fn output_exports_keep_their_own_images() {
    let v = vault(&[
        ("views/p/v.md", "# P\n\n![[views/p/g.png]]\n".to_string()),
        ("views/p/g.png", "AAA".to_string()),
        ("views/q/v.md", "# Q\n\n![[views/q/g.png]]\n".to_string()),
        ("views/q/g.png", "BBB".to_string()),
    ]);
    for (view, out) in [("views/p/v", "out/p.md"), ("views/q/v", "out/q.md")] {
        let (code, env) = v.run(&["export", view, "-o", out]);
        assert_eq!(code, 0, "{env:#}");
    }
    assert!(v.read("out/p.md").contains("](p.assets/g.png)"), "{}", v.read("out/p.md"));
    assert_eq!(v.read("out/p.assets/g.png"), "AAA");
    assert_eq!(v.read("out/q.assets/g.png"), "BBB");
}

#[test]
fn export_does_not_write_through_a_symlink() {
    // `.loci/` is gitignored, so `Vault::open` does not see the link.
    let v = vault(&[("views/plan.md", "# Plan\n".to_string()), (".gitignore", ".loci/\n".to_string())]);
    let elsewhere = v.tmp.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::create_dir_all(v.root.join(".loci")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, v.root.join(".loci/export")).unwrap();
    let (code, env) = v.run(&["export", "views/plan"]);
    assert_eq!((code, error_code(&env)), (1, "file.symlink"), "{env:#}");
    assert!(std::fs::read_dir(&elsewhere).unwrap().next().is_none(), "nothing written through the link");
}

#[test]
fn links_of_a_root_note_named_like_a_slug() {
    let v = vault(&[("index.md", "# Home\n\n[[views/w]]\n".to_string()), ("views/w.md", "# W\n".to_string())]);
    let (code, env) = v.run(&["links", "index.md"]);
    assert_eq!(code, 0, "{env:#}");
    assert_eq!(env["data"]["outgoing"][0]["target"], "views/w.md", "{env:#}");
}

#[test]
fn an_ambiguous_slug_is_refused() {
    let v = vault(&[
        ("concepts/g/bfs.md", concept(IDS[0], "", "# BFS\n")),
        ("concepts/h/bfs.md", concept(IDS[1], "", "# BFS\n")),
    ]);
    for args in [&["closure", "bfs"][..], &["export", "bfs"], &["merge", "bfs", "concepts/g/bfs"]] {
        let (code, env) = v.run(args);
        assert_eq!((code, error_code(&env)), (1, "concept.ambiguous"), "{args:?}: {env:#}");
    }
    let (code, env) = v.run(&["closure", "concepts/h/bfs"]);
    assert_eq!(code, 0, "a path is never ambiguous: {env:#}");
}

#[test]
fn links_lists_outgoing_and_incoming() {
    let v = vault(&[
        (
            "concepts/g/bfs.md",
            concept(
                IDS[0],
                "prereqs: [\"[[queue]]\"]\n",
                "# BFS\n\n## Proof\n\nSee [[missing]] and [[sources/papers/p.pdf#p2]].\n\n![[concepts/g/attachments/grid.png]]\n",
            ),
        ),
        ("concepts/g/queue.md", concept(IDS[1], "", "# Queue\n")),
        ("concepts/g/graph.md", concept(IDS[2], "", "# Graph\n\nWalked by BFS.\n")),
        ("views/v.md", "# V\n\n[[bfs#Proof]] and [[concepts/g/bfs]].\n".into()),
        ("concepts/g/attachments/grid.png", "PNG".into()),
        ("sources/papers/p.pdf", "%PDF".into()),
    ]);
    let (code, env) = v.run(&["links", "bfs"]);
    assert_eq!(code, 0, "{env:#}");
    let d = &env["data"];
    assert_eq!(d["note"], "concepts/g/bfs.md");
    let out: Vec<(String, serde_json::Value)> = d["outgoing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["form"].as_str().unwrap().to_string(), l["target"].clone()))
        .collect();
    assert_eq!(
        out,
        [
            ("prereq".into(), json!("concepts/g/queue.md")),
            ("wiki".into(), json!(null)),
            ("wiki".into(), json!("sources/papers/p.pdf")),
            ("embed".into(), json!("concepts/g/attachments/grid.png")),
        ]
    );
    // Incoming, fragments ignored: both view links.
    let incoming: Vec<(&str, u64)> = d["incoming"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["path"].as_str().unwrap(), l["line"].as_u64().unwrap()))
        .collect();
    assert_eq!(incoming, [("views/v.md", 3), ("views/v.md", 3)]);
    // A note that names it without linking it is a candidate for a link.
    assert_eq!(d["unlinked_in"], json!([{"path": "concepts/g/graph.md", "line": 6, "text": "Walked by BFS."}]));
    let (_, env) = v.run(&["links", "concepts/g/graph"]);
    assert_eq!(env["data"]["unlinked_out"], json!([{"line": 6, "text": "BFS", "concept": "concepts/g/bfs.md"}]));
    let (_, human, _) = v.run_raw(&["links", "bfs"]);
    assert!(human.contains("-> concepts/g/queue.md (prereq)\n"), "{human}");
    assert!(
        human.contains("unlinked mentions of concepts/g/bfs.md:\n  concepts/g/graph.md:6: Walked by BFS.\n"),
        "{human}"
    );

    // A prerequisite is an incoming link; a source is cited by an ordinary link; an image is embedded.
    let (_, env) = v.run(&["links", "queue"]);
    assert_eq!(env["data"]["incoming"][0]["path"], "concepts/g/bfs.md");
    assert_eq!(env["data"]["incoming"][0]["form"], "prereq");
    let (_, env) = v.run(&["links", "sources/papers/p.pdf"]);
    assert_eq!(env["data"]["incoming"][0]["path"], "concepts/g/bfs.md", "{env:#}");
    let (_, env) = v.run(&["links", "concepts/g/attachments/grid.png"]);
    assert_eq!(env["data"]["incoming"][0]["form"], "embed", "{env:#}");
    assert_eq!(env["data"]["outgoing"], json!([]));

    let (code, env) = v.run(&["links", "views/nothing"]);
    assert_eq!((code, error_code(&env)), (1, "note.unknown"));
}
