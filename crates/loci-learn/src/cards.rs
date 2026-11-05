//! Cards (learn-plugin §1): top-level list items under `## Cards` in concept notes.

use std::ops::Range;

use loci_core::id::Ulid;
use loci_core::index::Index;
use loci_core::parse::{Block, Note, Spanned};
use loci_core::path::VaultPath;

const SECTION: &str = "Cards";

/// A card (`of_note` spans it with its list item).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// Concept note path.
    pub path: VaultPath,
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

/// Every card in the vault, by note path, then in note order.
pub fn all(index: &Index) -> Vec<Spanned<Card>> {
    index.concepts().flat_map(|note| of_note(note).0).collect()
}

/// The cards of `note`, and the bytes of its `## Cards` sections that are no card (a
/// second section, a block other than a list, an item that is not `question :: answer`),
/// which `loci learn lint` reports.
pub fn of_note(note: &Note) -> (Vec<Spanned<Card>>, Vec<Range<usize>>) {
    let found = note.h2_sections(SECTION);
    let mut issues = vec![];
    if let Some(second) = found.get(1) {
        issues.push(second.heading.span.clone());
    }

    // Each item of a list at the top level of the section is a card; anything else
    // there is a format issue.
    let mut items = vec![];
    for block in found.iter().flat_map(|s| &s.blocks) {
        match &block.node {
            Block::List(list) => items.extend(list),
            _ => issues.push(block.span.clone()),
        }
    }

    let mut cards = vec![];
    for item in items {
        let id = item.id.as_ref();
        // The item's content, after its list marker, without its block id.
        let (start, end) = (item.marker.span.end, item.span.end);
        let raw = match id {
            Some(id) => format!("{}{}", note.text[start..id.span.start].trim_end(), &note.text[id.span.end..end]),
            None => note.text[start..end].to_string(),
        };
        match split(&raw) {
            Some((question, answer)) => {
                let card = Card {
                    path: note.path.clone(),
                    id: CardId::of(id.map(|id| id.value(&note.text))),
                    question,
                    answer,
                };
                cards.push(Spanned::new(card, item.span.clone()));
            }
            None => issues.push(item.span.clone()),
        }
    }

    (cards, issues)
}

/// Splits an item's content (after its list marker) at the first `::` in its text,
/// outside code spans and math, that is preceded by whitespace and followed by
/// whitespace or a line end (§1.1). Returns the question and the answer, trimmed.
fn split(raw: &str) -> Option<(String, String)> {
    use pulldown_cmark::{Event, Parser};
    const SEP: &str = "::";
    let bytes = raw.as_bytes();
    let text = Parser::new_ext(raw, loci_core::parse::options())
        .into_offset_iter()
        .filter_map(|(e, r)| matches!(e, Event::Text(_)).then_some(r));
    let at = text.flat_map(|r| raw[r.clone()].match_indices(SEP).map(move |(i, _)| r.start + i)).find(|&i| {
        let before_ws = i > 0 && bytes[i - 1].is_ascii_whitespace();
        before_ws && bytes.get(i + SEP.len()).is_none_or(|b| b.is_ascii_whitespace())
    })?;
    Some((raw[..at].trim().to_string(), raw[at + SEP.len()..].trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use loci_core::parse::Note;

    fn cards(body: &str) -> (Vec<Spanned<Card>>, Vec<Range<usize>>) {
        let text = format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n---\n# T\n\n{body}");
        of_note(&Note::parse(&VaultPath::fixed("concepts/a/x.md"), text))
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
    fn only_top_level_items_are_cards() {
        let (cs, issues) = cards("## Cards\n\n> - q :: a\n");
        assert!(cs.is_empty(), "an item of a quoted list is not a top-level item");
        assert_eq!(issues.len(), 1, "the quote is the issue");
    }

    #[test]
    fn the_question_starts_after_the_list_marker() {
        let (cs, _) = cards("## Cards\n\n1. q1 :: a1\n2. [ ] q2 :: a2\n");
        let qs: Vec<&str> = cs.iter().map(|c| c.question.as_str()).collect();
        assert_eq!(qs, ["q1", "q2"], "a task box is part of the marker");
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
