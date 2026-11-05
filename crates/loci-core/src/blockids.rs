//! `loci block-ids` (m0 §9.9): assign ULID block ids to the top-level blocks of a section.

use std::collections::BTreeSet;

use crate::envelope::Refusal;
use crate::id::Ulid;
use crate::parse::{Block, Note};
use crate::splice::{self, Edit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assigned {
    /// 1-based line where the id was inserted.
    pub line: usize,
    pub id: Ulid,
}

/// Computes the new text and the assigned ids. Refuses a note without the section
/// (`block-ids.no-section`) and ids that would change the blocks they mark
/// (`block-ids.changed`).
pub fn assign(
    note: &Note,
    section: &str,
    mut new_id: impl FnMut() -> Ulid,
) -> Result<(String, Vec<Assigned>), Refusal> {
    let sections = note.h2_sections(section);
    if sections.is_empty() {
        return Err(Refusal::refused("block-ids.no-section", format!("`{}` has no `## {section}`", note.path)));
    }

    let blocks: Vec<_> = sections.iter().flat_map(|s| s.blocks.iter().copied()).collect();
    let text = &note.text;

    // Where each new id goes: the end of the leading text of an item without one (an
    // item that starts with a nested block, a quote or a fence, has none to carry it),
    // and the end of a paragraph without one.
    let items = blocks.iter().flat_map(|b| match &b.node {
        Block::List(items) => &items[..],
        _ => &[],
    });
    let item_ends = items.filter(|i| i.id.is_none() && !i.lead.span.is_empty()).map(|i| i.lead.span.end);
    let paragraph_ends = blocks
        .iter()
        .filter(|b| matches!(b.node, Block::Paragraph { id: None }))
        .map(|b| b.span.start + b.text(text).trim_end().len());
    let mut ends: Vec<usize> = item_ends.chain(paragraph_ends).collect();
    ends.sort();

    let mut edits = vec![];
    let mut assigned = vec![];
    for at in ends {
        let id = new_id();
        assigned.push(Assigned { line: crate::envelope::line_col(text, at).0, id });
        edits.push(Edit::insert(at, format!(" ^{id}")));
    }

    let out = splice::apply(text, edits).expect("insertions at distinct block ends never overlap");
    refuse_changed(note, &out, &assigned, section)?;
    Ok((out, assigned))
}

/// An id must not change what it marks (a paragraph's last line `*` becomes a list item
/// once `^id` follows it): `out` has `note`'s blocks, each assigned id a block id.
fn refuse_changed(note: &Note, out: &str, assigned: &[Assigned], section: &str) -> Result<(), Refusal> {
    let after = crate::parse::Markdown::parse(out.to_string());
    let ids: BTreeSet<&str> = after.block_ids().map(|b| b.id.value(&after.text)).collect();
    let marked = assigned.iter().all(|a| ids.contains(a.id.to_string().as_str()));
    if marked && after.same_shape(note) {
        return Ok(());
    }
    let message = format!("block ids in `## {section}` would change its blocks; add them by hand");
    Err(Refusal::refused("block-ids.changed", message))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `k`th test id (1-based).
    fn id(k: usize) -> Ulid {
        format!("01ARZ3NDEKTSV4RRFFQ69G5F{k:02}").parse().unwrap()
    }

    /// Assigns ids, written as `ID1`, `ID2`, … in the returned text, and checks that
    /// assigning again changes nothing.
    fn run(text: &str) -> (String, Vec<Assigned>) {
        let n = crate::testing::concept(text);
        let mut k = 0;
        let (mut out, assigned) = assign(&n, "Cards", || {
            k += 1;
            id(k)
        })
        .unwrap();
        let again = assign(&crate::testing::concept(&out), "Cards", || unreachable!("no new target"));
        let (again, more) = again.unwrap();
        assert_eq!((again.as_str(), more.len()), (out.as_str(), 0), "assigning is idempotent");
        for i in 1..=assigned.len() {
            out = out.replace(&id(i).to_string(), &format!("ID{i}"));
        }
        (out, assigned)
    }

    #[test]
    fn ids_that_would_change_the_blocks_are_refused() {
        let t = "# T\n\n## Cards\n\nSteps follow\n*\n";
        let refused = assign(&crate::testing::concept(t), "Cards", || id(1)).unwrap_err();
        assert_eq!(refused.code(), "block-ids.changed");
        // A box-only item has no lead (pulldown-cmark ends it at its line), so `Answer
        // text` is a paragraph after the list and takes the id.
        let (out, _) = run("# T\n\n## Cards\n\n- [ ]\nAnswer text\n");
        assert_eq!(out, "# T\n\n## Cards\n\n- [ ]\nAnswer text ^ID1\n");
    }

    #[test]
    fn items_without_leading_text_are_skipped() {
        let t = "# T\n\n## Cards\n\n- > quoted q :: a\n- ```\n  code\n  ```\n- q :: a\n";
        let (out, ids) = run(t);
        assert_eq!(ids.len(), 1);
        assert_eq!(out, t.replace("- q :: a\n", "- q :: a ^ID1\n"));
    }

    #[test]
    fn quoted_paragraphs_are_not_targets() {
        let (out, ids) = run("# T\n\n## Cards\n\nTop.\n\n> Quoted.\n");
        assert_eq!(ids.len(), 1);
        assert_eq!(out, "# T\n\n## Cards\n\nTop. ^ID1\n\n> Quoted.\n");
    }

    #[test]
    fn top_level_items_only_in_section() {
        let t =
            "# T\n\nintro\n\n## Cards\n\n- q1 :: a1\n- q2 ::\n  - sub\n  - sub2\n- q3 :: a ^old\n\n## Other\n\n- no\n";
        let (out, ids) = run(t);
        assert_eq!(
            out,
            "# T\n\nintro\n\n## Cards\n\n- q1 :: a1 ^ID1\n- q2 :: ^ID2\n  - sub\n  - sub2\n- q3 :: a ^old\n\n## Other\n\n- no\n"
        );
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn loose_list_and_paragraph() {
        let t = "# T\n\n## Cards\n\n- q1 :: a1\n\n- q2 :: a2\n  continued\n\nA paragraph.\n";
        let (out, _) = run(t);
        assert_eq!(out, "# T\n\n## Cards\n\n- q1 :: a1 ^ID1\n\n- q2 :: a2\n  continued ^ID2\n\nA paragraph. ^ID3\n");
    }

    #[test]
    fn every_matching_section() {
        let t = "# T\n\n## Cards\n\n- a\n\n## X\n\n## Cards\n\n- b\n";
        let (out, ids) = run(t);
        assert_eq!(ids.len(), 2);
        assert!(out.contains("- a ^ID1\n") && out.contains("- b ^ID2\n"));
    }

    #[test]
    fn missing_section() {
        let n = crate::testing::concept("# T\n");
        assert_eq!(assign(&n, "Cards", || unreachable!("no section")).unwrap_err().code(), "block-ids.no-section");
    }
}
