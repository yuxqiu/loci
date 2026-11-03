//! `loci merge` (m1-m4 §1): fold one concept into another after the agent merged the
//! content: rewrite links and delete the absorbed note.

use std::collections::BTreeMap;

use crate::envelope::Refusal;
use crate::index::Index;
use crate::path::VaultPath;
use crate::relink::{self, Change};

/// Plans merging concept `from` into concept `into` (each a concept path or slug).
pub fn plan(index: &Index, from: &VaultPath, into: &VaultPath) -> Result<Merge, Refusal> {
    let concept = |arg: &VaultPath| index.concept(arg, "merge.not-concept");
    let (from, into) = (concept(from)?, concept(into)?);
    let (from, into) = (from.path.clone(), into.path.clone());
    if from == into {
        return Err(Refusal::refused("merge.same", "a concept cannot be merged into itself"));
    }

    // A prereq of `into` that named `from` would name `into` itself: `link_edits`
    // removes it.
    let targets: BTreeMap<VaultPath, VaultPath> = [(from.clone(), into.clone())].into();
    let mut change = Change { deleted: [from.clone()].into(), ..Change::default() };
    let edits = relink::link_edits(index, &targets, &mut change);
    let change = relink::finish(index, change, &targets, edits, "merge.verify")?;
    Ok(Merge { from, into, change })
}

/// A planned merge: the two concepts' paths and the change.
pub struct Merge {
    /// The absorbed concept, deleted by the change.
    pub from: VaultPath,
    pub into: VaultPath,
    pub change: Change,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::from_texts;

    fn vp(s: &str) -> VaultPath {
        VaultPath::new(s.to_string()).unwrap()
    }

    #[test]
    fn merge_rewrites_links_and_self_prereq() {
        let from = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TA\n---\n# A\n";
        let into = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TB\nprereqs: [\"[[old]]\", \"[[heap]]\"]\n---\n# B\n";
        let i = from_texts(&[
            ("concepts/a/old.md", from),
            ("concepts/a/dijkstra.md", into),
            ("concepts/a/heap.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TC\n---\n# H\n"),
            ("views/v.md", "# V\n\n[[old]] [[concepts/a/old#A]]\n"),
        ]);
        let c = plan(&i, &vp("old"), &vp("dijkstra")).unwrap().change;
        assert!(c.deleted.contains("concepts/a/old.md"));
        // Not a concept: named by its path, not reported as missing.
        let e = plan(&i, &vp("views/v"), &vp("dijkstra")).err().unwrap();
        assert_eq!(e.error.message, "`views/v.md` is not a concept");
        assert_eq!(c.writes["views/v.md"].text, "# V\n\n[[dijkstra]] [[concepts/a/dijkstra#A]]\n");
        let d = &c.writes["concepts/a/dijkstra.md"].text;
        assert!(d.contains("prereqs:\n  - \"[[heap]]\"\n"), "{d}");
    }

    #[test]
    fn adjacent_self_prereqs_are_removed_together() {
        let id = |n: &str| format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T{n}\n---\n# {n}\n");
        let c = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TC\nprereqs: [\"[[w]]\", \"[[y]]\", \"[[y]]\"]\n---\n# C\n";
        let i =
            from_texts(&[("concepts/a/w.md", id("W")), ("concepts/a/y.md", id("Y")), ("concepts/a/c.md", c.into())]);
        let d = &plan(&i, &vp("y"), &vp("c")).unwrap().change.writes["concepts/a/c.md"].text;
        assert!(d.contains("prereqs:\n  - \"[[w]]\"\n"), "{d}");
    }

    #[test]
    fn emptied_block_prereqs_are_dropped() {
        let into = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T2\nprereqs:\n  - \"[[old]]\"\n---\n# Into\n";
        let i = from_texts(&[
            ("concepts/a/old.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T1\n---\n# Old\n"),
            ("concepts/a/into.md", into),
        ]);
        let c = plan(&i, &vp("old"), &vp("into")).unwrap().change;
        assert_eq!(c.writes["concepts/a/into.md"].text, "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T2\n---\n# Into\n");
    }

    #[test]
    fn refusals() {
        let c = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TA\n---\n# A\n";
        let i = from_texts(&[("concepts/a/x.md", c), ("views/v.md", "# V\n")]);
        assert_eq!(plan(&i, &vp("x"), &vp("x")).err().unwrap().code().to_string(), "merge.same");
        assert_eq!(plan(&i, &vp("x"), &vp("views/v")).err().unwrap().code().to_string(), "merge.not-concept");
    }
}
