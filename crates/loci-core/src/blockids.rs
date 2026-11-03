//! `loci block-ids` (m0 §9.9): assign ULID block ids to the top-level blocks of a section.

use std::ops::Range;

use crate::id::Ulid;
use crate::parse::{BlockType, Note};
use crate::splice::{self, Edit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assigned {
    /// 1-based line where the id was inserted.
    pub line: usize,
    pub id: Ulid,
}

/// Computes the new text and the assigned ids. Returns `None` when no section matches.
pub fn assign(note: &Note, heading: &str, mut new_id: impl FnMut() -> Ulid) -> Option<(String, Vec<Assigned>)> {
    if note.h2_sections(heading).is_empty() {
        return None;
    }

    let (items, blocks) = note.section_top_level(heading);
    let text = &note.text;

    // Where each new id goes: the end of the leading text of an item without one (an
    // item that starts with a nested block, a quote or a fence, has none to carry it),
    // and the end of a paragraph without one.
    let item_ends = items.into_iter().filter(|i| i.id.is_none() && !i.lead.span.is_empty()).map(|i| i.lead.span.end);
    let has_id = |block: &Range<usize>| note.blocks.iter().any(|b| &b.span == block);
    let paragraph_ends = blocks
        .into_iter()
        .filter(|b| b.node == BlockType::Paragraph && !has_id(&b.span))
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
    Some((out, assigned))
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
        let n = crate::parse::concept(text);
        let mut k = 0;
        let (mut out, assigned) = assign(&n, "Cards", || {
            k += 1;
            id(k)
        })
        .unwrap();
        let (again, more) = assign(&crate::parse::concept(&out), "Cards", || unreachable!("no new target")).unwrap();
        assert_eq!((again.as_str(), more.len()), (out.as_str(), 0), "assigning is idempotent");
        for i in 1..=assigned.len() {
            out = out.replace(&id(i).to_string(), &format!("ID{i}"));
        }
        (out, assigned)
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
        let n = crate::parse::concept("# T\n");
        assert!(assign(&n, "Cards", || unreachable!("no section")).is_none());
    }
}
