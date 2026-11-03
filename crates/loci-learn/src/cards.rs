//! Cards (learn-plugin §1): top-level list items under `## Cards` in concept notes.

use std::ops::Range;

use loci_core::id::Ulid;
use loci_core::index::Index;
use loci_core::parse::{BlockKind, BlockType, Note};
use loci_core::path::VaultPath;

pub const SECTION: &str = "Cards";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// Concept note path.
    pub path: VaultPath,
    /// Byte range of the list item.
    pub range: Range<usize>,
    pub id: CardId,
    pub question: String,
    pub answer: String,
}

/// A card's block id (§1.2), as `loci learn lint` sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardId {
    /// No block id yet: `loci block-ids` assigns one.
    Missing,
    /// A block id that is not a ULID, as written.
    Malformed(String),
    Valid(Ulid),
}

impl CardId {
    fn of(written: Option<&str>) -> Self {
        match written {
            None => Self::Missing,
            Some(id) => id.parse().map_or_else(|_| Self::Malformed(id.to_string()), Self::Valid),
        }
    }

    /// The id, if it is a ULID.
    pub fn valid(&self) -> Option<Ulid> {
        match self {
            Self::Valid(id) => Some(*id),
            Self::Missing | Self::Malformed(_) => None,
        }
    }
}

/// A problem with a `## Cards` section, reported by `loci learn lint`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatIssue {
    pub path: VaultPath,
    pub range: Range<usize>,
}

/// Every card in the vault, by note path, then in note order, plus format issues.
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
        issues.push(FormatIssue { path: note.path.clone(), range: second.span.clone() });
    }

    let (items, blocks) = note.section_top_level(SECTION);

    // Anything at the top level of the section other than a list is a format issue.
    for range in blocks.into_iter().filter(|b| b.kind != BlockType::List).map(|b| &b.span) {
        issues.push(FormatIssue { path: note.path.clone(), range: range.clone() });
    }

    let mut cards = vec![];
    for item in items {
        let id = note.block_ids.iter().find(|b| b.kind == BlockKind::Item && b.block == item.span);
        // The item's content, after its list marker, without its block id (cut by its
        // byte range).
        let content = item.content_start..item.span.end;
        let mut raw = note.text[content.clone()].to_string();
        if let Some(b) = id {
            let (start, end) = (b.span.start - content.start, b.span.end - content.start);
            raw = format!("{}{}", raw[..start].trim_end(), &raw[end..]);
        }
        match split(&raw) {
            Some((question, answer)) => cards.push(Card {
                path: note.path.clone(),
                range: item.span.clone(),
                id: CardId::of(id.map(|b| b.id.as_str())),
                question,
                answer,
            }),
            None => issues.push(FormatIssue { path: note.path.clone(), range: item.span.clone() }),
        }
    }

    (cards, issues)
}

/// Splits an item's content (after its list marker) at the first `::` in its text,
/// outside code spans and math, that is preceded by whitespace and followed by
/// whitespace or a line end (§1.1). Returns the question and the answer, trimmed.
fn split(raw: &str) -> Option<(String, String)> {
    use pulldown_cmark::{Event, Parser};
    let bytes = raw.as_bytes();
    let text = Parser::new_ext(raw, loci_core::parse::options())
        .into_offset_iter()
        .filter_map(|(e, r)| matches!(e, Event::Text(_)).then_some(r));
    let at = text.flat_map(|r| raw[r.clone()].match_indices("::").map(move |(i, _)| r.start + i)).find(|&i| {
        let before_ws = i > 0 && bytes[i - 1].is_ascii_whitespace();
        before_ws && bytes.get(i + 2).is_none_or(|b| b.is_ascii_whitespace())
    })?;
    Some((raw[..at].trim().to_string(), raw[at + 2..].trim().to_string()))
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
        assert_eq!(split("q :: a"), Some(("q".into(), "a".into())));
        assert_eq!(split("q ::\n  a"), Some(("q".into(), "a".into())));
        assert_eq!(split("`std::vector` x :: a"), Some(("`std::vector` x".into(), "a".into())));
        assert_eq!(split("`a :: b` c"), None);
        assert_eq!(split("$a :: b$ c"), None, "math is not text");
        assert_eq!(split("a::b"), None);
        // An unmatched backtick is literal text (CommonMark), not an open code span.
        assert_eq!(split("what is ` :: a backtick"), Some(("what is `".into(), "a backtick".into())));
    }

    #[test]
    fn the_question_starts_after_the_list_marker() {
        let (cs, _) = cards("## Cards\n\n1. q1 :: a1\n2. [ ] q2 :: a2\n");
        let qs: Vec<&str> = cs.iter().map(|c| c.question.as_str()).collect();
        assert_eq!(qs, ["q1", "[ ] q2"]);
    }

    #[test]
    fn tight_loose_nested() {
        let id1 = "01JBA2K7N4P8R3T6W9X2Z5C8DF";
        let id2 = "01JB9XR2M8C4V6T0Q3W5H7K9PA";
        let (tight, issues) = cards(&format!("## Cards\n\n- q1 :: a1 ^{id1}\n- q2 :: ^{id2}\n  - x\n  - y\n"));
        assert!(issues.is_empty());
        assert_eq!(tight.len(), 2);
        assert_eq!(tight[0].id, CardId::Valid(id1.parse().unwrap()));
        assert_eq!(tight[0].answer, "a1");
        assert_eq!(tight[1].id, CardId::Valid(id2.parse().unwrap()));
        assert!(tight[1].answer.contains("- x"));
        let (loose, _) = cards(&format!("## Cards\n\n- q1 :: a1 ^{id1}\n\n- q2 :: a2 ^{id2}\n"));
        assert_eq!(
            loose.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
            tight.iter().map(|c| c.id.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn card_ids() {
        let (cs, _) = cards("## Cards\n\n- q1 :: a1\n- q2 :: a2 ^old\n");
        assert_eq!(
            cs.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
            [CardId::Missing, CardId::Malformed("old".into())]
        );
    }

    #[test]
    fn format_issues() {
        let (_, issues) = cards("## Cards\n\nA paragraph.\n\n- no separator\n\n## Cards\n\n- q :: a\n");
        assert_eq!(issues.len(), 3);
    }
}
