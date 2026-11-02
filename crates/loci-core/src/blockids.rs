//! `loci block-ids` (m0 §9.9): assign ULID block ids to the top-level blocks of a section.

use std::ops::Range;

use crate::id::Ulid;
use crate::parse::{Note, TopBlock};
use crate::splice::{self, Edit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assigned {
    /// 1-based line of the block's id slice end.
    pub line: usize,
    pub id: Ulid,
}

/// Computes the new text and the assigned ids. Returns `None` when no section matches.
pub fn assign(note: &Note, heading: &str, mut new_id: impl FnMut() -> Ulid) -> Option<(String, Vec<Assigned>)> {
    if note.h2_sections(heading).is_empty() {
        return None;
    }
    let (items, blocks) = note.section_top_level(heading);
    let has_id = |block: &Range<usize>| note.block_ids.iter().any(|b| &b.block == block);

    // Targets: top-level list items (their leading text) and top-level paragraphs. An
    // item that starts with a nested block (a quote, a fence) has no leading text to
    // carry an id.
    let mut targets: Vec<(Range<usize>, Range<usize>)> = vec![]; // (block, id slice)
    for item in items.into_iter().filter(|i| !note.text[i.content_start.min(i.lead.end)..i.lead.end].trim().is_empty())
    {
        if !has_id(&item.range) {
            targets.push((item.range.clone(), item.lead.clone()));
        }
    }
    for (_, r) in blocks.into_iter().filter(|(b, _)| matches!(b, TopBlock::Paragraph)) {
        if !has_id(r) {
            targets.push((r.clone(), r.clone()));
        }
    }
    targets.sort_by_key(|(b, _)| b.start);

    let text = &note.text;
    let mut edits = vec![];
    let mut assigned = vec![];
    for (_, slice) in targets {
        let at = slice.start + text[slice.clone()].trim_end().len();
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
    use crate::parse::parse;
    use crate::path::VaultPath;
    use crate::vault::Kind;

    /// The `k`th test id (1-based).
    fn id(k: usize) -> Ulid {
        format!("01ARZ3NDEKTSV4RRFFQ69G5F{k:02}").parse().unwrap()
    }

    /// Assigns ids, written as `ID1`, `ID2`, … in the returned text.
    fn run(text: &str) -> (String, Vec<Assigned>) {
        let n = parse(&VaultPath::new("concepts/a/x.md".into()).unwrap(), Kind::Concept, text.to_string());
        let mut k = 0;
        let (mut out, assigned) = assign(&n, "Cards", || {
            k += 1;
            id(k)
        })
        .unwrap();
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
        let again = parse(&VaultPath::new("concepts/a/x.md".into()).unwrap(), Kind::Concept, out.clone());
        let (twice, more) = assign(&again, "Cards", || unreachable!("no new target")).unwrap();
        assert!(more.is_empty());
        assert_eq!(twice, out);
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
        // Idempotent.
        let n = parse(&VaultPath::new("concepts/a/x.md".into()).unwrap(), Kind::Concept, out.clone());
        let (again, more) = assign(&n, "Cards", || unreachable!("no new target")).unwrap();
        assert_eq!(again, out);
        assert!(more.is_empty());
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
        let n = parse(&VaultPath::new("concepts/a/x.md".into()).unwrap(), Kind::Concept, "# T\n".into());
        assert!(assign(&n, "Cards", || unreachable!("no section")).is_none());
    }
}
