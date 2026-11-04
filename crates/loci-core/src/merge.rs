//! `loci merge` (m1-m4 §1): fold one concept into another after the agent merged the
//! content: rewrite links and delete the absorbed note.

use std::collections::BTreeMap;

use crate::envelope::Refusal;
use crate::index::Index;
use crate::parse::Note;
use crate::path::VaultPath;
use crate::relink::Change;

impl Merge {
    /// Plans merging concept `from` into concept `into`.
    pub fn plan(index: &Index, from: &Note, into: &Note) -> Result<Merge, Refusal> {
        let (from, into) = (from.path.clone(), into.path.clone());
        if from == into {
            return Err(Refusal::refused("merge.same", "a concept cannot be merged into itself"));
        }

        // A prereq of `into` that named `from` would name `into` itself: it is removed.
        let absorbed = [(from.clone(), into.clone())].into();
        let change = Change::relink(index, BTreeMap::new(), absorbed, "merge.verify")?;
        Ok(Merge { from, into, change })
    }
}

/// A planned merge: the two concepts' paths and the change.
#[derive(Debug)]
pub struct Merge {
    /// The absorbed concept, deleted by the change.
    pub from: VaultPath,
    pub into: VaultPath,
    pub change: Change,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp(s: &str) -> VaultPath {
        VaultPath::new(s.to_string()).unwrap()
    }

    fn note<'i>(index: &'i Index, path: &str) -> &'i Note {
        index.parsed(&vp(path)).unwrap()
    }

    #[test]
    fn merge_rewrites_links_and_self_prereq() {
        let from = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TA\n---\n# A\n";
        let into = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TB\nprereqs: [\"[[old]]\", \"[[heap]]\"]\n---\n# B\n";
        let i = crate::testing::index(&[
            ("concepts/a/old.md", from),
            ("concepts/a/dijkstra.md", into),
            ("concepts/a/heap.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TC\n---\n# H\n"),
            ("views/v.md", "# V\n\n[[old]] [[concepts/a/old#A]]\n"),
        ]);
        let c = Merge::plan(&i, note(&i, "concepts/a/old.md"), note(&i, "concepts/a/dijkstra.md")).unwrap().change;
        assert!(c.deleted.contains("concepts/a/old.md"));
        assert_eq!(c.writes["views/v.md"].text, "# V\n\n[[dijkstra]] [[concepts/a/dijkstra#A]]\n");
        let d = &c.writes["concepts/a/dijkstra.md"].text;
        assert!(d.contains("prereqs:\n  - \"[[heap]]\"\n"), "{d}");
    }

    #[test]
    fn adjacent_self_prereqs_are_removed_together() {
        let id = |n: &str| format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T{n}\n---\n# {n}\n");
        let c = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TC\nprereqs: [\"[[w]]\", \"[[y]]\", \"[[y]]\"]\n---\n# C\n";
        let i = crate::testing::index(&[
            ("concepts/a/w.md", id("W")),
            ("concepts/a/y.md", id("Y")),
            ("concepts/a/c.md", c.into()),
        ]);
        let d =
            &Merge::plan(&i, note(&i, "concepts/a/y.md"), note(&i, "concepts/a/c.md")).unwrap().change.writes["concepts/a/c.md"]
                .text;
        assert!(d.contains("prereqs:\n  - \"[[w]]\"\n"), "{d}");
    }

    #[test]
    fn emptied_block_prereqs_are_dropped() {
        let into = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T2\nprereqs:\n  - \"[[old]]\"\n---\n# Into\n";
        let i = crate::testing::index(&[
            ("concepts/a/old.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T1\n---\n# Old\n"),
            ("concepts/a/into.md", into),
        ]);
        let c = Merge::plan(&i, note(&i, "concepts/a/old.md"), note(&i, "concepts/a/into.md")).unwrap().change;
        assert_eq!(c.writes["concepts/a/into.md"].text, "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T2\n---\n# Into\n");
    }
}
