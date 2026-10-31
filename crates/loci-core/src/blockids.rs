//! `loci block-ids` (m0 §9.10): assign ULID block ids to the top-level blocks of a section.

use std::ops::Range;

use crate::parse::{ListItem, Note, TopBlock};
use crate::splice::{self, Edit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assigned {
    /// 1-based line of the block's id slice end.
    pub line: usize,
    pub id: String,
}

/// The byte ranges of the sections under every H2 whose text is `heading`, each up to
/// the next H1 or H2.
pub fn sections(note: &Note, heading: &str) -> Vec<Range<usize>> {
    let mut out = vec![];
    for (i, h) in note.headings.iter().enumerate() {
        if h.level != 2 || h.text != heading {
            continue;
        }
        let end = note.headings[i + 1..].iter().find(|n| n.level <= 2).map_or(note.text.len(), |n| n.range.start);
        out.push(h.range.end..end);
    }
    out
}

/// Fresh ULIDs, monotonic within one run (m0 §9.10).
pub fn ulid_ids() -> impl FnMut() -> String {
    let mut generator = ulid::Generator::new();
    move || generator.generate().expect("a ULID generator does not overflow within one run").to_string()
}

/// Computes the new text and the assigned ids. Returns `None` when no section matches.
pub fn assign(note: &Note, heading: &str, mut new_id: impl FnMut() -> String) -> Option<(String, Vec<Assigned>)> {
    let sections = sections(note, heading);
    if sections.is_empty() {
        return None;
    }
    let inside = |r: &Range<usize>| sections.iter().any(|s| s.start <= r.start && r.end <= s.end);
    let has_id = |block: &Range<usize>| note.block_ids.iter().any(|b| &b.block == block);

    // Targets: top-level list items (their leading text) and top-level paragraphs.
    let mut targets: Vec<(Range<usize>, Range<usize>)> = vec![]; // (block, id slice)
    // An item that starts with a nested block (a quote, a fence) has no leading text to
    // carry an id.
    let has_lead = |item: &ListItem| {
        let lead = note.text[item.lead.clone()].trim_start();
        let marker = lead.trim_start_matches(|c: char| c.is_ascii_digit());
        let rest = marker.strip_prefix(['-', '*', '+', '.', ')']).unwrap_or(marker);
        !rest.trim().is_empty()
    };
    for item in note.items.iter().filter(|i| i.depth == 1 && inside(&i.range) && has_lead(i)) {
        if !has_id(&item.range) {
            targets.push((item.range.clone(), item.lead.clone()));
        }
    }
    for (_, r) in note.top_blocks.iter().filter(|(b, r)| matches!(b, TopBlock::Paragraph) && inside(r)) {
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
        assigned.push(Assigned { line: crate::envelope::line_col(text, at).0, id: id.clone() });
        edits.push(Edit::insert(at, format!(" ^{id}")));
    }
    let out = splice::apply(text, edits).expect("insertions at distinct block ends never overlap");
    Some((out, assigned))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;
    use crate::vault::Kind;

    fn run(text: &str) -> (String, Vec<Assigned>) {
        let n = parse("concepts/a/x.md", Kind::Concept, text.to_string());
        let mut k = 0;
        assign(&n, "Cards", || {
            k += 1;
            format!("ID{k}")
        })
        .unwrap()
    }

    #[test]
    fn items_without_leading_text_are_skipped() {
        let t = "# T\n\n## Cards\n\n- > quoted q :: a\n- ```\n  code\n  ```\n- q :: a\n";
        let (out, ids) = run(t);
        assert_eq!(ids.len(), 1);
        assert_eq!(out, t.replace("- q :: a\n", "- q :: a ^ID1\n"));
        let again = parse("concepts/a/x.md", Kind::Concept, out.clone());
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
        let n = parse("concepts/a/x.md", Kind::Concept, out.clone());
        let (again, more) = assign(&n, "Cards", || "X".into()).unwrap();
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
        let n = parse("concepts/a/x.md", Kind::Concept, "# T\n".into());
        assert!(assign(&n, "Cards", || "X".into()).is_none());
    }
}
