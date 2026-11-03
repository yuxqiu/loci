//! Block ids (m0 §4.6): a `^id` ending a list item's lead (nested items too) or a paragraph.

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Item,
    Paragraph,
}

/// A block with a block id, spanning the block (an item with its nested items, or a
/// paragraph).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    /// The id, without its `^`, spanning `^id`.
    pub id: Spanned<String>,
}

/// A `^id` ending a text: the marker (`^id`) and the id.
static BLOCK_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|\s)(\^(\w+))\s*$").unwrap());

/// Block ids (§4.6), in file order: an item's ends its lead; a paragraph's ends a
/// paragraph (`paragraphs` holds none that is an item's lead).
pub(super) fn blocks(text: &str, items: &[Spanned<ListItem>], paragraphs: &[Range<usize>]) -> Vec<Spanned<Block>> {
    let mut out = vec![];
    for item in items {
        if let Some(id) = trailing_id(text, &item.lead.span) {
            out.push(Spanned::new(Block { kind: BlockKind::Item, id }, item.span.clone()));
        }
    }

    for p in paragraphs {
        if let Some(id) = trailing_id(text, p) {
            out.push(Spanned::new(Block { kind: BlockKind::Paragraph, id }, p.clone()));
        }
    }

    out.sort_by_key(|b| b.id.span.start);
    out
}

/// The `^id` ending the text at `range`: the id, spanning `^id`.
fn trailing_id(text: &str, range: &Range<usize>) -> Option<Spanned<String>> {
    let caps = BLOCK_ID.captures(&text[range.clone()])?;
    let (marker, id) = (caps.get(1)?, caps.get(2)?);
    Some(Spanned::new(id.as_str().to_string(), range.start + marker.start()..range.start + marker.end()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Note, parse};
    use crate::path::VaultPath;
    use crate::vault::Kind;

    fn p(text: &str) -> Note {
        parse(&VaultPath::new("concepts/a/x.md".into()).unwrap(), Kind::Concept, text.to_string())
    }

    #[test]
    fn block_ids_tight_and_loose() {
        let t = "# T\n\n- one ^aaa\n- two\n  continued ^bbb\n\npara ^ccc\n";
        let n = p(t);
        let ids: Vec<_> = n.blocks.iter().map(|b| (b.id.as_str(), b.kind)).collect();
        assert_eq!(ids, vec![("aaa", BlockKind::Item), ("bbb", BlockKind::Item), ("ccc", BlockKind::Paragraph)]);
        let loose = "# T\n\n- one ^aaa\n\n- two ^bbb\n";
        let n = p(loose);
        let ids: Vec<_> = n.blocks.iter().map(|b| (b.id.as_str(), b.kind)).collect();
        assert_eq!(ids, vec![("aaa", BlockKind::Item), ("bbb", BlockKind::Item)]);
    }

    #[test]
    fn block_id_before_nested_list() {
        let t = "# T\n\n- parent ^ppp\n  - child ^ccc\n- next\n";
        let n = p(t);
        let ids: Vec<_> = n.blocks.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, vec!["ppp", "ccc"]);
        let parent = &n.items[0];
        assert_eq!(parent.lead.text(t), "parent ^ppp");
        assert_eq!(n.items[1].depth, 2);
    }
}
