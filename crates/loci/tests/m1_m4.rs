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
        ("concepts/g/bfs.md", concept(IDS[0], "", "Level order.", "# BFS\n\n## Queue\n\nText.\n")),
        (
            "concepts/g/graph-search.md",
            concept(IDS[1], "prereqs: [\"[[bfs]]\"]\n", "Search.", "# Graph search\n\nSee [[bfs]].\n"),
        ),
        (
            "concepts/g/dijkstra.md",
            concept(
                IDS[2],
                "prereqs: [\"[[bfs]]\"]\n",
                "Weighted.",
                "# Dijkstra\n\n[[concepts/g/bfs#Queue|the queue]] and [[bfs]].\n",
            ),
        ),
    ]);
    let (code, env) = v.run(&["merge", "bfs", "graph-search"]);
    assert_eq!(code, 0, "{env}");
    assert!(!v.exists("concepts/g/bfs.md"));
    let into = v.read("concepts/g/graph-search.md");
    assert!(
        !into.contains("prereqs: [\"[[bfs]]\"]") && !into.contains("[[graph-search]]\"]"),
        "self-prereq removed:\n{into}"
    );
    assert!(!into.contains("aliases"), "no aliases are added:\n{into}");
    assert!(into.contains("See [[graph-search]]."));
    let d = v.read("concepts/g/dijkstra.md");
    assert!(d.contains("prereqs: [\"[[graph-search]]\"]"), "{d}");
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
    let v = vault(&[("concepts/g/bfs.md", concept(IDS[0], "", "d", "# BFS\n")), ("views/plan.md", "# Plan\n".into())]);
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
        ("concepts/g/top.md", concept(IDS[0], &p(&["mid", "base", "nope"]), "Top.", "# Top\n")),
        ("concepts/g/mid.md", concept(IDS[1], &p(&["base"]), "Mid.", "# Mid\n")),
        ("concepts/g/base.md", concept(IDS[2], "", "Base.", "# Base\n")),
    ]);
    let (code, env) = v.run(&["closure", "top"]);
    assert_eq!(code, 0, "{env}");
    let c = &env["data"]["closure"];
    let order: Vec<_> =
        c.as_array().unwrap().iter().map(|m| (m["slug"].as_str().unwrap(), m["depth"].as_u64().unwrap())).collect();
    assert_eq!(order, [("base", 1), ("mid", 1)]);
    assert_eq!(c[0]["digest"], "Base.");
    assert_eq!(env["data"]["unresolved"], json!([{"from": "concepts/g/top.md", "target": "nope"}]));
    let (_, human, _) = v.run_raw(&["closure", "top"]);
    assert_eq!(human, "- [[base]]: Base.\n- [[mid]]: Mid.\n");

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
        ("concepts/g/a.md", concept(IDS[0], &p("b"), "A.", "# A\n")),
        ("concepts/g/b.md", concept(IDS[1], &p("c"), "B.", "# B\n")),
        ("concepts/g/c.md", concept(IDS[2], &p("b"), "C.", "# C\n")),
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
fn bench_tool_and_agent_levels() {
    let v = vault(&[
        ("concepts/g/bfs.md", concept(IDS[0], "", "Breadth-first search.", "# BFS\n\nVisits by level with a queue.\n")),
        ("views/plan.md", "# Plan\n\nNothing relevant.\n".into()),
        (
            "bench/retrieval.jsonl",
            "{\"q\": \"queue level\", \"expect\": [\"bfs\"]}\n\n{\"q\": \"zebra\", \"expect\": [\"views/plan.md\"]}\n"
                .into(),
        ),
    ]);
    // The test PATH holds only `git`: the agent cannot start.
    let (code, env) = v.run(&["bench", "--agent", "echo 'answer: [[bfs]]'"]);
    assert_eq!((code, error_code(&env)), (3, "bench.agent"));
    std::os::unix::fs::symlink(find_on_path("sh"), v.bin().join("sh")).unwrap();
    let (code, env) = v.run(&["bench", "--agent", "echo 'answer: [[bfs]]'"]);
    assert_eq!(code, 0, "{env}");
    let d = &env["data"];
    assert_eq!(d["cases"], 2, "{env}");
    assert_eq!(d["recall_at_k"], 0.5, "{env}");
    assert_eq!(d["mrr"], 0.5);
    assert_eq!(d["results"][0]["rank"], 1);
    assert!(d["results"][1]["rank"].is_null());
    assert_eq!(d["agent"]["pass_rate"], 0.5);
    assert_eq!(d["results"][0]["agent_pass"], true);
    assert_eq!(d["results"][1]["agent_pass"], false);

    // Output larger than a pipe buffer does not block the agent.
    std::os::unix::fs::symlink(find_on_path("head"), v.bin().join("head")).unwrap();
    let start = std::time::Instant::now();
    let (code, env) = v.run(&["bench", "--agent", "head -c 200000 /dev/zero; echo '[[bfs]]'"]);
    assert_eq!(code, 0, "{env:#}");
    assert_eq!(env["data"]["agent"]["pass_rate"], 0.5);
    assert!(start.elapsed() < std::time::Duration::from_secs(30));

    let (_, env) = v.run(&["bench"]);
    assert!(env["data"]["agent"].is_null());
    assert!(env["data"]["results"][0]["agent_pass"].is_null());

    v.write("bench/retrieval.jsonl", "{\"q\": \"x\", \"expect\": []}\n{\"q\": \"x\"}\n");
    let (code, env) = v.run(&["bench"]);
    assert_eq!((code, error_code(&env)), (1, "bench.case"));
    assert!(env["error"]["message"].as_str().unwrap().contains("line 2"), "{env}");

    std::fs::remove_file(v.root.join("bench/retrieval.jsonl")).unwrap();
    let (code, env) = v.run(&["bench"]);
    assert_eq!((code, error_code(&env)), (1, "bench.none"));
}

#[test]
fn export_collects_links_glossary_and_assets() {
    let v = vault(&[
        (
            "views/study.md",
            "# Study\n\nStart with [[bfs]], then [[dijkstra#Relaxation]].\n\n![[assets/g/grid.png]]\n".into(),
        ),
        (
            "concepts/g/bfs.md",
            concept(IDS[0], "", "Level order.", "# BFS\n\n## Queue\n\nUses a queue; see [[graph]].\n"),
        ),
        (
            "concepts/g/dijkstra.md",
            concept(
                IDS[1],
                "",
                "Weighted.",
                "# Dijkstra\n\nIntro.\n\n## Relaxation\n\nRelax edges.\n\n## Other\n\nNot exported.\n",
            ),
        ),
        ("concepts/g/graph.md", concept(IDS[2], "", "Vertices and edges.", "# Graph\n\nBody.\n")),
        ("assets/g/grid.png", "PNG".into()),
    ]);
    let (code, env) = v.run(&["export", "views/study"]);
    assert_eq!(code, 0, "{env}");
    let d = &env["data"];
    assert_eq!(d["output"].as_str().unwrap(), v.root.join(".loci/export/study.md").to_str().unwrap());
    assert_eq!(strs(&d["notes"]), ["views/study.md", "concepts/g/bfs.md", "concepts/g/dijkstra.md"]);
    assert_eq!(strs(&d["glossary"]), ["concepts/g/graph.md"], "depth 1: graph is two links away");
    assert_eq!(strs(&d["assets"]), ["assets/grid.png"]);
    assert_eq!(v.read(".loci/export/assets/grid.png"), "PNG");
    let md = v.read(".loci/export/study.md");
    assert!(!md.contains("id: "), "frontmatter stripped:\n{md}");
    assert!(md.contains("Relax edges.") && !md.contains("Not exported."), "only the linked section:\n{md}");
    assert!(md.contains("](#"), "links to included notes become anchors:\n{md}");
    assert!(md.contains("## Glossary") && md.contains("**Graph**: Vertices and edges."), "{md}");
    assert!(md.contains("![](assets/grid.png)") || md.contains("](assets/grid.png)"), "{md}");

    // Depth 2 pulls graph in; nothing left for the glossary.
    let (_, env) = v.run(&["export", "views/study", "--depth", "2"]);
    assert!(strs(&env["data"]["notes"]).contains(&"concepts/g/graph.md"));
    assert!(strs(&env["data"]["glossary"]).is_empty());

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

#[test]
fn export_html_to_relative_paths() {
    use std::os::unix::fs::PermissionsExt;
    let v = vault(&[("views/plan.md", "# Plan\n\nText.\n".into())]);
    // A stub pandoc: fails unless its input exists, then copies it to `-o`.
    let stub = v.bin().join("pandoc");
    std::fs::write(&stub, "#!/bin/sh\nin=$1\ntest -f \"$in\" || exit 2\nwhile [ \"$1\" != -o ]; do shift; done\nexec cat \"$in\" > \"$2\"\n")
        .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    for tool in ["sh", "cat"] {
        std::os::unix::fs::symlink(find_on_path(tool), v.bin().join(tool)).unwrap();
    }
    for out in ["out/plan.html", "plan.html"] {
        let (code, env) = v.run(&["export", "views/plan", "--to", "html", "-o", out]);
        assert_eq!(code, 0, "{out}: {env:#}");
        assert!(v.read(out).contains("Text."), "{out}");
        assert!(!v.root.join(out.replace(".html", ".md.tmp")).exists(), "{out}: the temporary file is removed");
    }
}

#[test]
fn an_ambiguous_slug_is_refused() {
    let v = vault(&[
        ("concepts/g/bfs.md", concept(IDS[0], "", "A.", "# BFS\n")),
        ("concepts/h/bfs.md", concept(IDS[1], "", "B.", "# BFS\n")),
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
                "prereqs: [\"[[queue]]\"]\nsources: [\"sources/papers/p#p2\"]\n",
                "d",
                "# BFS\n\n## Proof\n\nSee [[missing]].\n\n![[concepts/g/attachments/grid.png]]\n",
            ),
        ),
        ("concepts/g/queue.md", concept(IDS[1], "", "d", "# Queue\n")),
        ("views/v.md", "# V\n\n[[bfs#Proof]] and [[concepts/g/bfs]].\n".into()),
        ("concepts/g/attachments/grid.png", "PNG".into()),
        ("sources/papers/p/p.pdf", "%PDF".into()),
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
            ("source".into(), json!("sources/papers/p")),
            ("wiki".into(), json!(null)),
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

    // A prerequisite is an incoming link; a source file is cited through its folder; an image is embedded.
    let (_, env) = v.run(&["links", "queue"]);
    assert_eq!(env["data"]["incoming"][0]["path"], "concepts/g/bfs.md");
    assert_eq!(env["data"]["incoming"][0]["form"], "prereq");
    let (_, env) = v.run(&["links", "sources/papers/p/p.pdf"]);
    assert_eq!(env["data"]["incoming"][0]["form"], "source", "{env:#}");
    let (_, env) = v.run(&["links", "concepts/g/attachments/grid.png"]);
    assert_eq!(env["data"]["incoming"][0]["form"], "embed", "{env:#}");
    assert_eq!(env["data"]["outgoing"], json!([]));

    let (code, env) = v.run(&["links", "views/nothing"]);
    assert_eq!((code, error_code(&env)), (1, "links.unknown"));
}

#[test]
fn lint_links_and_mv_agree_on_sources_entries() {
    let v = vault(&[
        ("concepts/g/x.md", concept(IDS[0], "sources: [\"journal/r/day.md\", \"concepts/g\"]\n", "d", "# X\n")),
        ("journal/r/day.md", "# Day\n".into()),
    ]);
    let (_, env) = v.run(&["lint"]);
    let unresolved: Vec<&str> =
        with_code(&env, "sources.unresolved").iter().map(|d| d["message"].as_str().unwrap()).collect();
    assert_eq!(unresolved.len(), 1, "{env:#}");
    assert!(unresolved[0].contains("concepts/g"), "a concept folder is not a source");
    let (_, env) = v.run(&["links", "x"]);
    let targets: Vec<&serde_json::Value> =
        env["data"]["outgoing"].as_array().unwrap().iter().map(|l| &l["target"]).collect();
    assert_eq!(targets, [&json!("journal/r/day.md"), &json!(null)]);
    assert_eq!(env["data"]["incoming"], json!([]), "a note does not link to itself through its folder");
    let (code, env) = v.run(&["mv", "journal/r/day.md", "journal/r/day2.md"]);
    assert_eq!(code, 0, "{env:#}");
    assert!(v.read("concepts/g/x.md").contains("\"journal/r/day2\""), "{}", v.read("concepts/g/x.md"));
    let (_, env) = v.run(&["lint"]);
    assert_eq!(with_code(&env, "sources.unresolved").len(), 1, "mv keeps the journal source resolving");
}
