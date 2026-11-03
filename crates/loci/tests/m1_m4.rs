//! `merge`, `closure`, `bench` and `export` (docs/specs/m1-m4.md).

mod common;

use common::*;
use serde_json::json;

const IDS: [&str; 4] = [
    "01JB9XQ7Z3K4M5N6P7Q8R9S0A1",
    "01JB9XQ7Z3K4M5N6P7Q8R9S0A2",
    "01JB9XQ7Z3K4M5N6P7Q8R9S0A3",
    "01JB9XQ7Z3K4M5N6P7Q8R9S0A4",
];

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
    assert_eq!((code, error_code(&env)), (1, "merge.not-concept"));
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
    assert_eq!((code, error_code(&env)), (1, "closure.unknown"));
    // A concept path is accepted in the same forms as elsewhere (`./`, `.md`).
    let (code, env) = v.run(&["closure", "./concepts/g/top.md"]);
    assert_eq!(code, 0, "{env:#}");
    let (code, env) = v.run(&["closure", "../top"]);
    assert_eq!((code, error_code(&env)), (1, "path.outside"));
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
    assert!(env["error"]["message"].as_str().unwrap().contains("line 2"), "{env}");

    v.write("bench/cases.jsonl", "\n");
    let (code, env) = v.run(&["bench", "true"]);
    assert_eq!((code, error_code(&env)), (1, "bench.none"), "no cases");
    std::fs::remove_file(v.root.join("bench/cases.jsonl")).unwrap();
    let (code, env) = v.run(&["bench", "true"]);
    assert_eq!((code, error_code(&env)), (1, "bench.none"));
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
    assert_eq!(d["output"].as_str().unwrap(), v.root.join(".loci/export/study.md").to_str().unwrap());
    assert_eq!(strs(&d["notes"]), ["views/study.md", "concepts/g/bfs.md", "concepts/g/dijkstra.md"]);
    assert_eq!(strs(&d["assets"]), ["assets/grid.png"]);
    assert_eq!(v.read(".loci/export/assets/grid.png"), "PNG");
    let md = v.read(".loci/export/study.md");
    assert!(!md.contains("id: "), "frontmatter stripped:\n{md}");
    assert!(md.contains("Relax edges.") && !md.contains("Not exported."), "only the linked section:\n{md}");
    assert!(md.contains("](#"), "links to included notes become anchors:\n{md}");
    assert!(md.contains("see Graph.") && !md.contains("Glossary"), "a note two links away is plain text:\n{md}");
    assert!(md.contains("![](assets/grid.png)") || md.contains("](assets/grid.png)"), "{md}");

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
    assert_eq!((code, error_code(&env)), (1, "export.unknown"));
    let (code, env) = v.run(&["export", "areas/g/AREA.md"]);
    assert_eq!((code, error_code(&env)), (1, "export.kind"));
}

/// HTML is rendered by loci itself and needs no network: every `#anchor` link points at
/// a heading `id` in the page, math is MathML, and `-o` is relative to the working
/// directory.
#[test]
fn export_html_is_rendered_in_process() {
    let v = vault(&[
        ("views/plan.md", "# Plan\n\n## Proof\n\nSee [[bfs]].\n".into()),
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
        assert_eq!(ids, ["plan", "proof", "bfs", "proof-1"], "{page}");
        assert!(!links.is_empty() && links.iter().all(|l| ids.contains(l)), "{links:?} in {ids:?}");
        assert_eq!(page.matches("<math").count(), 2, "{page}");
        assert!(page.contains("<msub>") && page.contains("O(V+E)"), "math is MathML, source kept: {page}");
        assert!(!page.contains("http"), "nothing from the network: {page}");
    }
    assert!(!v.exists("assets"), "no images, no assets folder");
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
    assert_eq!((code, error_code(&env)), (1, "links.unknown"));
}
