//! Cards (learn-plugin §1): top-level list items under `## Cards` in concept notes.

use std::ops::Range;

use loci_core::index::Index;
use loci_core::parse::{BlockKind, Note, TopBlock, strip_list_marker};
use loci_core::path::VaultPath;

pub const SECTION: &str = "Cards";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// Concept note path.
    pub path: VaultPath,
    /// Byte range of the list item.
    pub range: Range<usize>,
    /// Position in the note (0-based, across `## Cards` sections).
    pub position: usize,
    pub id: Option<String>,
    pub question: String,
    pub answer: String,
}

/// A problem with a `## Cards` section, reported by `loci learn lint`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatIssue {
    pub path: VaultPath,
    pub range: Range<usize>,
}

/// Every card in the vault, in note-path order then position, plus format issues.
pub fn all(index: &Index) -> (Vec<Card>, Vec<FormatIssue>) {
    let mut cards = vec![];
    let mut issues = vec![];
    for note in index.concepts() {
        let (c, i) = of_note(note);
        cards.extend(c);
        issues.extend(i);
    }
    (cards, issues)
}

pub fn of_note(note: &Note) -> (Vec<Card>, Vec<FormatIssue>) {
    let found = note.h2_sections(SECTION);
    let mut issues = vec![];
    if let Some((second, _)) = found.get(1) {
        issues.push(FormatIssue { path: note.path.clone(), range: second.range.clone() });
    }
    let (items, blocks) = note.section_top_level(SECTION);
    // Anything at the top level of the section other than a list is a format issue.
    for (_, range) in blocks.into_iter().filter(|(b, _)| !matches!(b, TopBlock::List)) {
        issues.push(FormatIssue { path: note.path.clone(), range: range.clone() });
    }
    let mut cards = vec![];
    for item in items {
        let id = note.block_ids.iter().find(|b| b.kind == BlockKind::Item && b.block == item.range);
        // The item's text without its block id (cut by its byte range).
        let mut raw = note.text[item.range.clone()].to_string();
        if let Some(b) = id {
            let (start, end) = (b.range.start - item.range.start, b.range.end - item.range.start);
            raw = format!("{}{}", raw[..start].trim_end(), &raw[end..]);
        }
        match split(&raw) {
            Some((question, answer)) => cards.push(Card {
                path: note.path.clone(),
                range: item.range.clone(),
                position: cards.len(),
                id: id.map(|b| b.id.clone()),
                question,
                answer,
            }),
            None => issues.push(FormatIssue { path: note.path.clone(), range: item.range.clone() }),
        }
    }
    (cards, issues)
}

/// Splits an item's raw text at the first `::` outside inline code that is preceded by
/// whitespace and followed by whitespace or a line end (§1.1). Returns the question and
/// the answer, trimmed, with the list marker removed from the question.
fn split(raw: &str) -> Option<(String, String)> {
    let bytes = raw.as_bytes();
    let mut i = 0;
    let mut code_ticks: Option<usize> = None; // backtick run length of an open code span
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let run = bytes[i..].iter().take_while(|b| **b == b'`').count();
            match code_ticks {
                None => code_ticks = Some(run),
                Some(n) if n == run => code_ticks = None,
                Some(_) => {}
            }
            i += run;
            continue;
        }
        if code_ticks.is_none() && bytes[i..].starts_with(b"::") {
            let before_ws = i > 0 && bytes[i - 1].is_ascii_whitespace();
            let after = bytes.get(i + 2);
            let after_ok = after.is_none_or(|b| b.is_ascii_whitespace());
            if before_ws && after_ok {
                let q = strip_list_marker(raw[..i].trim());
                let a = raw[i + 2..].trim();
                return Some((q.to_string(), a.to_string()));
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use loci_core::parse::parse;
    use loci_core::vault::Kind;

    fn cards(body: &str) -> (Vec<Card>, Vec<FormatIssue>) {
        let text = format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n---\n# T\n\n{body}");
        of_note(&parse(&VaultPath::new("concepts/a/x.md".into()).unwrap(), Kind::Concept, text))
    }

    #[test]
    fn splits() {
        assert_eq!(split("- q :: a"), Some(("q".into(), "a".into())));
        assert_eq!(split("- q ::\n  a"), Some(("q".into(), "a".into())));
        assert_eq!(split("- `std::vector` x :: a"), Some(("`std::vector` x".into(), "a".into())));
        assert_eq!(split("- `a :: b` c"), None);
        assert_eq!(split("- a::b"), None);
        assert_eq!(split("1. q :: a"), Some(("q".into(), "a".into())));
    }

    #[test]
    fn tight_loose_nested() {
        let id1 = "01JBA2K7N4P8R3T6W9X2Z5C8DF";
        let id2 = "01JB9XR2M8C4V6T0Q3W5H7K9PA";
        let (tight, issues) = cards(&format!("## Cards\n\n- q1 :: a1 ^{id1}\n- q2 :: ^{id2}\n  - x\n  - y\n"));
        assert!(issues.is_empty());
        assert_eq!(tight.len(), 2);
        assert_eq!(tight[0].id.as_deref(), Some(id1));
        assert_eq!(tight[0].answer, "a1");
        assert_eq!(tight[1].id.as_deref(), Some(id2));
        assert!(tight[1].answer.contains("- x"));
        let (loose, _) = cards(&format!("## Cards\n\n- q1 :: a1 ^{id1}\n\n- q2 :: a2 ^{id2}\n"));
        assert_eq!(
            loose.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
            tight.iter().map(|c| c.id.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn format_issues() {
        let (_, issues) = cards("## Cards\n\nA paragraph.\n\n- no separator\n\n## Cards\n\n- q :: a\n");
        assert_eq!(issues.len(), 3);
    }
}
