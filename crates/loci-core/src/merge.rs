//! `loci merge` (m1-m4 §1): fold one concept into another after the agent merged the
//! content: rewrite links and delete the absorbed note.

use std::collections::BTreeMap;

use crate::envelope::Refusal;
use crate::index::Index;
use crate::parse::Note;
use crate::relink::Change;

/// Plans merging concept `from` into concept `into`: the change deletes `from`.
pub fn plan(index: &Index, from: &Note, into: &Note) -> Result<Change, Refusal> {
    if from.path == into.path {
        return Err(Refusal::refused("merge.same", "a concept cannot be merged into itself"));
    }

    // A prereq of `into` that named `from` would name `into` itself: it is removed.
    let absorbed = [(from.path.clone(), into.path.clone())].into();
    Change::relink(index, BTreeMap::new(), absorbed, "merge.verify")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::VaultPath;

    fn vp(s: &str) -> VaultPath {
        VaultPath::new(s.to_string()).unwrap()
    }

    fn note<'i>(index: &'i Index, path: &str) -> &'i Note {
        index.parsed(&vp(path)).unwrap()
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
            &plan(&i, note(&i, "concepts/a/y.md"), note(&i, "concepts/a/c.md")).unwrap().writes["concepts/a/c.md"].text;
        assert!(d.contains("prereqs:\n  - \"[[w]]\"\n"), "{d}");
    }
}
