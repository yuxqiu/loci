//! `loci merge` (m1-m4 §1): fold one concept into another after the agent merged the
//! content: rewrite links, add aliases, delete the absorbed note.

use std::collections::{BTreeMap, BTreeSet};

use crate::envelope::Refusal;
use crate::index::{Index, Resolved};
use crate::parse::{FmValue, LinkForm, Note};
use crate::relink::{self, Change};
use crate::vault;

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
    let into_note = &index.notes[&into];
    let self_prereqs = self_prereqs(index, into_note, &from);
    let mut into_edits = edits.remove(&into).unwrap_or_default();
    into_edits.retain(|e| !self_prereqs.iter().any(|(_, r)| r.contains(&e.range.start)));
    if let Some(entry) = into_note.frontmatter.as_ref().and_then(|f| f.get("prereqs"))
        && let FmValue::List { items, span, flow } = &entry.value
        && !self_prereqs.is_empty()
    {
        if !*flow && self_prereqs.len() == items.len() {
            // An emptied block list would leave `prereqs:` (null): drop the whole entry.
            let last = items.last().expect("the list holds the removed items").span();
            into_edits.push(relink::remove_lines(&into_note.text, entry.key_span.start..last.end));
        } else {
            for (_, item_range) in &self_prereqs {
                into_edits.push(relink::remove_list_item(into_note, span.clone(), item_range.clone(), *flow));
            }
        }
        change.removed_links.extend(self_prereqs.iter().map(|(i, _)| (into.clone(), *i)));
    }

    // `from`'s slug and aliases that `into` lacks become aliases of `into`.
    let mut have: BTreeSet<String> = aliases(into_note).into_iter().map(|a| a.to_lowercase()).collect();
    have.insert(vault::stem(&into).to_lowercase());
    let mut new_aliases = vec![vault::stem(&from).to_string()];
    new_aliases.extend(aliases(&index.notes[&from]));
    // Keep the first spelling of each name `into` does not already have.
    new_aliases.retain(|a| have.insert(a.to_lowercase()));
    if !new_aliases.is_empty() {
        into_edits.push(relink::alias_edit(into_note, &new_aliases, "merge.frontmatter")?);
        change.aliases_added.extend(new_aliases.into_iter().map(|a| (into.clone(), a)));
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
        .filter(|(_, l)| matches!(index.resolve(into, l), Ok(Resolved::Note(p)) if p == from))
        .map(|(i, l)| (i, l.range.clone()))
        .collect()
}

fn aliases(note: &Note) -> Vec<String> {
    note.fm_strings("aliases").into_iter().map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::from_texts;

    #[test]
    fn merge_rewrites_links_aliases_and_self_prereq() {
        let from = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TA\naliases: [sssp]\ndigest: d\n---\n# A\n";
        let into = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TB\nprereqs: [\"[[old]]\", \"[[heap]]\"]\ndigest: d\n---\n# B\n";
        let i = from_texts(&[
            ("concepts/a/old.md", from),
            ("concepts/a/dijkstra.md", into),
            ("concepts/a/heap.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TC\ndigest: d\n---\n# H\n"),
            ("views/v.md", "# V\n\n[[old]] [[concepts/a/old#A]]\n"),
        ]);
        let c = plan(&i, "old", "dijkstra").unwrap().change;
        assert!(c.deleted.contains("concepts/a/old.md"));
        assert_eq!(c.writes["views/v.md"], "# V\n\n[[dijkstra]] [[concepts/a/dijkstra#A]]\n");
        let d = &c.writes["concepts/a/dijkstra.md"];
        assert!(d.contains("prereqs: [\"[[heap]]\"]"), "{d}");
        assert!(d.contains("aliases: [old, sssp]"), "{d}");
    }

    #[test]
    fn emptied_block_prereqs_are_dropped() {
        let into = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T2\nprereqs:\n  - \"[[old]]\"\ndigest: d\n---\n# Into\n";
        let i = from_texts(&[
            ("concepts/a/old.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T1\ndigest: d\n---\n# Old\n"),
            ("concepts/a/into.md", into),
        ]);
        let c = plan(&i, "old", "into").unwrap().change;
        assert_eq!(
            c.writes["concepts/a/into.md"],
            "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0T2\naliases: [old]\ndigest: d\n---\n# Into\n"
        );
    }

    #[test]
    fn refusals() {
        let c = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TA\ndigest: d\n---\n# A\n";
        let i = from_texts(&[("concepts/a/x.md", c), ("views/v.md", "# V\n")]);
        assert_eq!(plan(&i, "x", "x").err().unwrap().code().to_string(), "merge.same");
        assert_eq!(plan(&i, "x", "views/v").err().unwrap().code().to_string(), "merge.not-concept");
    }
}
