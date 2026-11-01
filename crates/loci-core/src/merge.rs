//! `loci merge` (m1-m4 §1): fold one concept into another after the agent merged the
//! content: rewrite links and delete the absorbed note.

use std::collections::BTreeMap;

use crate::envelope::Refusal;
use crate::index::Index;
use crate::parse::{FmValue, LinkForm, Note};
use crate::relink::{self, Change};
use crate::splice::Edit;

/// Plans merging concept `from` into concept `into` (each a concept path or slug).
pub fn plan(index: &Index, from: &str, into: &str) -> Result<Merge, Refusal> {
    let concept = |arg: &str| index.concept(arg).map_err(|e| e.refusal("merge.not-concept", arg));
    let (from, into) = (concept(from)?, concept(into)?);
    let (from, into) = (from.path.clone(), into.path.clone());
    if from == into {
        return Err(Refusal::refused("merge.same", "a concept cannot be merged into itself"));
    }
    let targets: BTreeMap<String, String> = [(from.clone(), into.clone())].into();
    let mut change = Change { deleted: [from.clone()].into(), ..Change::default() };
    let mut edits = relink::link_edits(index, &targets, &[from.clone()].into());

    // A prereq of `into` that named `from` would now name `into` itself: remove it.
    let into_note = index.parsed(&into).expect("`concept` resolved `into` to a readable note");
    let self_prereqs = self_prereqs(index, into_note, &from);
    let mut into_edits = edits.remove(&into).unwrap_or_default();
    into_edits.retain(|e| !self_prereqs.iter().any(|(_, r)| r.contains(&e.range.start)));
    if let Some(entry) = into_note.frontmatter.as_ref().and_then(|f| f.get("prereqs"))
        && let FmValue::List { items, flow, .. } = &entry.value
        && !self_prereqs.is_empty()
    {
        let text = &into_note.text;
        let removed = |item: &FmValue| self_prereqs.iter().any(|(_, r)| *r == item.span());
        let (first, last) = (items[0].span(), items.last().expect("the list holds the removed items").span());
        if *flow {
            // One edit for the whole list, so adjacent removals cannot overlap. Items are
            // verbatim (m0 §6.1), so their text is copied as written.
            let kept: Vec<&str> = items.iter().filter(|i| !removed(i)).map(|i| &text[i.span()]).collect();
            into_edits.push(Edit::replace(first.start..last.end, kept.join(", ")));
        } else if self_prereqs.len() == items.len() {
            // An emptied block list would leave `prereqs:` (null): drop the whole entry.
            into_edits.push(remove_lines(text, entry.key_span.start..last.end));
        } else {
            into_edits.extend(self_prereqs.iter().map(|(_, r)| remove_lines(text, r.clone())));
        }
        change.removed_links.extend(self_prereqs.iter().map(|(i, _)| (into.clone(), *i)));
    }

    if !into_edits.is_empty() {
        edits.insert(into.clone(), into_edits);
    }
    relink::add_writes(index, &mut change, edits)?;
    relink::verify(index, &targets, &change, "merge.verify")?;
    Ok(Merge { from, into, change })
}

/// A planned merge: the two concepts' paths and the change.
pub struct Merge {
    /// The absorbed concept, deleted by the change.
    pub from: String,
    pub into: String,
    pub change: Change,
}

/// `into`'s prereq links that resolve to `from`: (index into `links`, item span).
fn self_prereqs(index: &Index, into: &Note, from: &str) -> Vec<(usize, std::ops::Range<usize>)> {
    into.links
        .iter()
        .enumerate()
        .filter(|(_, l)| l.form == LinkForm::Prereq)
        .filter(|(_, l)| index.link_target(into, l).as_deref() == Some(from))
        .map(|(i, l)| (i, l.range.clone()))
        .collect()
}

/// The edit that removes the whole lines spanned by `range`.
fn remove_lines(text: &str, range: std::ops::Range<usize>) -> Edit {
    let line_start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[range.end..].find('\n').map_or(text.len(), |i| range.end + i + 1);
    Edit::replace(line_start..line_end, "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::from_texts;

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
        let c = plan(&i, "old", "dijkstra").unwrap().change;
        assert!(c.deleted.contains("concepts/a/old.md"));
        // Not a concept: named by its path, not reported as missing.
        let e = plan(&i, "views/v", "dijkstra").err().unwrap();
        assert_eq!(e.error.message, "`views/v.md` is not a concept");
        assert_eq!(c.writes["views/v.md"], "# V\n\n[[dijkstra]] [[concepts/a/dijkstra#A]]\n");
        let d = &c.writes["concepts/a/dijkstra.md"];
        assert!(d.contains("prereqs: [\"[[heap]]\"]"), "{d}");
    }

    #[test]
    fn adjacent_self_prereqs_are_removed_together() {
        let id = |n: &str| format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T{n}\n---\n# {n}\n");
        let c = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TC\nprereqs: [\"[[w]]\", \"[[y]]\", \"[[Y]]\"]\n---\n# C\n";
        let i =
            from_texts(&[("concepts/a/w.md", id("W")), ("concepts/a/y.md", id("Y")), ("concepts/a/c.md", c.into())]);
        let d = &plan(&i, "y", "c").unwrap().change.writes["concepts/a/c.md"];
        assert!(d.contains("prereqs: [\"[[w]]\"]\n"), "{d}");
    }

    #[test]
    fn emptied_block_prereqs_are_dropped() {
        let into = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T2\nprereqs:\n  - \"[[old]]\"\n---\n# Into\n";
        let i = from_texts(&[
            ("concepts/a/old.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T1\n---\n# Old\n"),
            ("concepts/a/into.md", into),
        ]);
        let c = plan(&i, "old", "into").unwrap().change;
        assert_eq!(c.writes["concepts/a/into.md"], "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T2\n---\n# Into\n");
    }

    #[test]
    fn refusals() {
        let c = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TA\n---\n# A\n";
        let i = from_texts(&[("concepts/a/x.md", c), ("views/v.md", "# V\n")]);
        assert_eq!(plan(&i, "x", "x").err().unwrap().code().to_string(), "merge.same");
        assert_eq!(plan(&i, "x", "views/v").err().unwrap().code().to_string(), "merge.not-concept");
    }
}
