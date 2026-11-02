//! Properties of m0 §13.3 over generated in-memory vaults, through the `loci-core`
//! library: `mv` keeps resolving links resolving to the mapped target; `merge` sends
//! every link into the absorbed concept to the one it was merged into and leaves the
//! rest; `lint` is deterministic (but for the fresh ids its fixes propose); splicing an empty edit list is the identity and one-pass splicing
//! equals applying edits one at a time from the end.

use std::collections::BTreeMap;

use loci_core::config::Config;
use loci_core::index::{self, Index};
use loci_core::lint;
use loci_core::path::VaultPath;
use loci_core::splice::{self, Edit};
use loci_core::{merge, mv};
use proptest::prelude::*;
use proptest::sample::subsequence;

type Files = BTreeMap<String, String>;

fn vp(s: &str) -> VaultPath {
    VaultPath::new(s.to_string()).unwrap()
}

fn build(files: &Files) -> Index {
    let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    index::from_texts(&files)
}

const SLUGS: &[&str] = &["alpha", "beta", "gamma-ray", "delta", "二分查找", "排序", "검색", "かな"];
const VIEWS: &[&str] = &["views/plan.md", "views/learn/alpha.md", "views/视图.md", "views/deep/x/y.md"];
const JOURNAL: &[&str] = &["journal/r/2025/03/25-03-14.md", "journal/r/2025/04/25-04-01-日记.md"];
const AREAS: &[&str] = &["a", "b", "算法"];

/// A link to `target` (a vault path) in one of the generated forms.
fn link_text(target: &str, slug_form: bool, style: u8) -> String {
    let t = if slug_form {
        vp(target).stem().to_string()
    } else {
        target.strip_suffix(".md").unwrap_or(target).to_string()
    };
    match style % 4 {
        0 => format!("[[{t}]]"),
        1 => format!("[[{t}|label]]"),
        2 => format!("\n\n| col | link |\n|---|---|\n| x | [[{t}\\|pipe label]] |\n\n"),
        _ => format!("- item [[{t}]] ^b{style}\n"),
    }
}

#[derive(Debug, Clone)]
struct GenVault {
    files: Files,
    /// Candidate move sources (files and directories).
    movable: Vec<String>,
}

fn vault() -> impl Strategy<Value = GenVault> {
    (
        subsequence(SLUGS.to_vec(), 1..=SLUGS.len()),
        prop::collection::vec((0usize..AREAS.len(), any::<bool>()), SLUGS.len()),
        subsequence(VIEWS.to_vec(), 0..=VIEWS.len()),
        subsequence(JOURNAL.to_vec(), 0..=JOURNAL.len()),
        prop::collection::vec(
            prop::collection::vec((any::<prop::sample::Index>(), any::<bool>(), any::<u8>()), 0..5),
            16,
        ),
        any::<bool>(),
    )
        .prop_map(|(slugs, placement, views, journal, links, prereqs)| {
            let mut paths: Vec<(String, bool)> = vec![]; // (path, is concept)
            for (i, s) in slugs.iter().enumerate() {
                let (area, sub) = placement[i];
                let dir =
                    if sub { format!("concepts/{}/sub", AREAS[area]) } else { format!("concepts/{}", AREAS[area]) };
                paths.push((format!("{dir}/{s}.md"), true));
            }
            for v in &views {
                paths.push((v.to_string(), false));
            }
            for j in &journal {
                paths.push((j.to_string(), false));
            }
            let mut files = Files::new();
            let concept_paths: Vec<String> = paths.iter().filter(|(_, c)| *c).map(|(p, _)| p.clone()).collect();
            for (n, (path, is_concept)) in paths.iter().enumerate() {
                let mut body = String::from("# Title\n\nText.\n\n");
                for (idx, slug_form, style) in &links[n % links.len()] {
                    let (target, target_is_concept) = &paths[idx.index(paths.len())];
                    body.push_str(&link_text(target, *slug_form && *target_is_concept, *style));
                    body.push_str(" more text\n\n");
                }
                let text = if *is_concept {
                    let pre = if prereqs && concept_paths.len() > 1 {
                        let other = concept_paths.iter().find(|p| *p != path).unwrap();
                        format!("prereqs: [\"[[{}]]\"]\n", vp(other).stem())
                    } else {
                        String::new()
                    };
                    format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S{n:03}\n{pre}---\n{body}")
                } else {
                    body
                };
                files.insert(path.clone(), text);
            }
            for a in AREAS {
                if files.keys().any(|p| p.starts_with(&format!("concepts/{a}/"))) {
                    files.insert(format!("concepts/{a}/AREA.md"), format!("# Area {a}\n\nParagraph.\n"));
                }
            }
            let mut movable: Vec<String> = paths.iter().map(|(p, _)| p.clone()).collect();
            movable.extend(["views".to_string(), "journal".to_string()]);
            for a in AREAS {
                movable.push(format!("concepts/{a}"));
                movable.push(format!("concepts/{a}/sub"));
            }
            movable.retain(|m| files.keys().any(|f| f == m || f.starts_with(&format!("{m}/"))));
            GenVault { files, movable }
        })
}

/// A destination for `from` that keeps concepts inside an area and is fresh.
fn destination(from: &str, rename: bool) -> String {
    if from.ends_with(".md") {
        let dir = from.rsplit_once('/').map_or("", |(d, _)| d);
        if from.starts_with("concepts/") {
            let new_dir = if rename { dir.to_string() } else { "concepts/moved-area/deeper".to_string() };
            let name = if rename { "renamed-新".to_string() } else { vp(from).stem().to_string() };
            format!("{new_dir}/{name}.md")
        } else {
            format!("{dir}/moved/renamed-新.md")
        }
    } else if from.starts_with("concepts/") {
        "concepts/moved-area".to_string()
    } else if !from.contains('/') {
        // A top-level folder moves inside itself, so its files stay in a known folder.
        format!("{from}/moved/inner")
    } else {
        format!("{from}-moved/inner")
    }
}

/// Applies a plan in memory.
fn apply_plan(files: &Files, plan: &loci_core::relink::Change) -> Files {
    let map: BTreeMap<&str, &str> = plan.moves.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let mut out = Files::new();
    for (p, t) in files.iter().filter(|(p, _)| !plan.deleted.contains(p.as_str())) {
        let new = map.get(p.as_str()).map_or(p.clone(), |n| n.to_string());
        let text = plan.writes.get(new.as_str()).map_or_else(|| t.clone(), |w| w.text.clone());
        out.insert(new, text);
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    #[test]
    fn mv_keeps_resolving_links_resolving(g in vault(), pick in any::<prop::sample::Index>(), rename in any::<bool>()) {
        let from = g.movable[pick.index(g.movable.len())].clone();
        let to = destination(&from, rename);
        let before = build(&g.files);
        let plan = match mv::plan(&before, &vp(&from), &vp(&to), |_| Ok(false), |_| false) {
            Ok(p) => p,
            Err(e) => {
                // A rename onto a slug in use is a legitimate refusal; anything else is not
                // (every generated destination follows the path rules).
                prop_assert_eq!(e.code(), "mv.collision", "mv {} {}: {}", from, to, e.error.message);
                return Ok(());
            }
        };
        let after_files = apply_plan(&g.files, &plan);
        let after = build(&after_files);
        let map: BTreeMap<&str, &str> = plan.moves.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let mapped = |p: &str| map.get(p).map_or(p.to_string(), |n| n.to_string());
        for note in before.notes() {
            let new_path = plan.moves.get(&note.path).unwrap_or(&note.path);
            let new_note = after.parsed(new_path).expect("every note survives a move");
            prop_assert_eq!(note.links.len(), new_note.links.len(), "{}", note.path);
            for (old, new) in note.links.iter().zip(&new_note.links) {
                let Ok(r) = before.resolve(note, old) else { continue };
                let r2 = after.resolve(new_note, new);
                prop_assert_eq!(
                    r2.as_ref().map(|x| x.path().to_string()),
                    Ok(mapped(r.path().as_str())),
                    "mv {} {}: `{}` in {} -> `{}`\n{}", from, to, old.raw, note.path, new.raw, new_note.text
                );
            }
        }
        // Lint finds no new link errors.
        let config = Config::parse("schema = 1\n").unwrap();
        let link_errors = |i: &Index| lint::run(i, &config)
            .into_iter()
            .filter(|d| d.code.starts_with("link.") || d.code == "prereqs.cycle")
            .count();
        prop_assert!(link_errors(&after) <= link_errors(&before));
    }

    #[test]
    fn merge_sends_links_to_the_concept_kept(g in vault(), a in any::<prop::sample::Index>(), b in any::<prop::sample::Index>()) {
        let before = build(&g.files);
        let concepts: Vec<VaultPath> = before.concepts().map(|n| n.path.clone()).collect();
        prop_assume!(concepts.len() >= 2);
        let from = concepts[a.index(concepts.len())].clone();
        let into = concepts[b.index(concepts.len())].clone();
        prop_assume!(from != into);
        let plan = match merge::plan(&before, &from, &into) {
            Ok(p) => p,
            // Two concepts that share a slug cannot be named by it; any other refusal is not
            // legitimate on these vaults.
            Err(e) => {
                prop_assert!(before.slug_paths(&from.slug().unwrap()).len() > 1, "merge {} {}: {}", from, into, e.error.message);
                return Ok(());
            }
        };
        let after = build(&apply_plan(&g.files, &plan.change));
        prop_assert!(after.parsed(&from).is_none(), "the absorbed note is gone");
        for note in before.notes().filter(|n| n.path != from && n.path != into && n.kind != loci_core::vault::Kind::Source) {
            let new_note = after.parsed(&note.path).expect("other notes stay");
            prop_assert_eq!(note.links.len(), new_note.links.len(), "{}", note.path);
            for (old, new) in note.links.iter().zip(&new_note.links) {
                let Some(target) = before.link_target(note, old) else { continue };
                let expected = if target == from { into.clone() } else { target };
                prop_assert_eq!(after.link_target(new_note, new), Some(expected), "`{}` in {}", old.raw, note.path);
            }
        }
        // The concept kept names no absorbed note, and no prerequisite of itself.
        let kept = after.parsed(&into).expect("the concept kept stays");
        prop_assert!(kept.links.iter().filter_map(|l| after.link_target(kept, l)).all(|t| t != from));
        prop_assert!(after.prereqs(kept).0.iter().all(|p| *p != into));
    }

    #[test]
    fn lint_is_deterministic(g in vault()) {
        let config = Config::parse("schema = 1\n").unwrap();
        let run = || {
            let i = build(&g.files);
            loci_testkit::redact_ulids(&serde_json::to_string(&lint::run(&i, &config)).unwrap())
        };
        prop_assert_eq!(run(), run());
    }

    #[test]
    fn splice_identity_and_one_pass(
        text in "[a-z \\n二分é]{0,40}",
        cuts in prop::collection::vec((any::<prop::sample::Index>(), any::<prop::sample::Index>(), "[x世\\n]{0,3}"), 0..6),
    ) {
        prop_assert_eq!(splice::apply(&text, vec![]).unwrap(), text.clone());
        let bounds: Vec<usize> = (0..=text.len()).filter(|i| text.is_char_boundary(*i)).collect();
        // Non-overlapping edits with distinct starts, as (start, end, new).
        let mut points: Vec<usize> = cuts.iter().flat_map(|(a, b, _)| [a.index(bounds.len()), b.index(bounds.len())]).collect();
        points.sort();
        points.dedup();
        // Consecutive pairs of sorted, distinct boundaries: strictly increasing starts,
        // possibly adjacent edits, a zero-length insert when a pair is incomplete.
        let mut edits = vec![];
        for (k, (_, _, new)) in cuts.iter().enumerate() {
            let Some(&s) = points.get(k * 2) else { break };
            let e = points.get(k * 2 + 1).copied().unwrap_or(s);
            edits.push(Edit::replace(bounds[s]..bounds[e], new.clone()));
        }
        let one = splice::apply(&text, edits.clone()).unwrap();
        let mut seq = text.clone();
        let mut from_end = edits;
        from_end.sort_by_key(|e| std::cmp::Reverse(e.range.start));
        for e in from_end {
            seq = splice::apply(&seq, vec![e]).unwrap();
        }
        prop_assert_eq!(one, seq);
    }
}
