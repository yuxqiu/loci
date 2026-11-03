//! Block ids (m0 §4.6): a `^id` ending a top-level list item's lead or a paragraph.

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Item,
    Paragraph,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockId {
    pub id: String,
    /// The bytes of `^id`.
    pub span: Range<usize>,
    pub block: Range<usize>,
    pub kind: BlockKind,
}

static BLOCK_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|\s)\^(\w+)\s*$").unwrap());

/// Block ids (§4.6), in file order. A paragraph that is an item's lead carries the
/// item's id.
pub(super) fn block_ids(text: &str, items: &[ListItem], paragraphs: &[Range<usize>]) -> Vec<BlockId> {
    let mut out = vec![];
    for item in items {
        if let Some(b) = block_id(text, item.lead.clone()) {
            out.push(BlockId { block: item.span.clone(), kind: BlockKind::Item, ..b });
        }
    }
    let leads: std::collections::HashSet<&Range<usize>> = items.iter().map(|i| &i.lead).collect();
    for p in paragraphs.iter().filter(|p| !leads.contains(p)) {
        if let Some(b) = block_id(text, p.clone()) {
            out.push(BlockId { kind: BlockKind::Paragraph, ..b });
        }
    }
    out.sort_by_key(|b| b.span.start);
    out
}

fn block_id(text: &str, range: Range<usize>) -> Option<BlockId> {
    let slice = &text[range.clone()];
    let trimmed = slice.trim_end();
    let caps = BLOCK_ID.captures(trimmed)?;
    let m = caps.get(1).unwrap();
    let start = range.start + m.start() - 1; // include the `^`
    Some(BlockId {
        id: m.as_str().to_string(),
        span: start..range.start + m.end(),
        block: range,
        kind: BlockKind::Paragraph,
    })
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
        let ids: Vec<_> = n.block_ids.iter().map(|b| (b.id.as_str(), b.kind)).collect();
        assert_eq!(ids, vec![("aaa", BlockKind::Item), ("bbb", BlockKind::Item), ("ccc", BlockKind::Paragraph)]);
        let loose = "# T\n\n- one ^aaa\n\n- two ^bbb\n";
        let n = p(loose);
        let ids: Vec<_> = n.block_ids.iter().map(|b| (b.id.as_str(), b.kind)).collect();
        assert_eq!(ids, vec![("aaa", BlockKind::Item), ("bbb", BlockKind::Item)]);
    }

    #[test]
    fn block_id_before_nested_list() {
        let t = "# T\n\n- parent ^ppp\n  - child ^ccc\n- next\n";
        let n = p(t);
        let ids: Vec<_> = n.block_ids.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, vec!["ppp", "ccc"]);
        let parent = &n.items[0];
        assert_eq!(&t[parent.lead.clone()].trim_end(), &"- parent ^ppp");
        assert_eq!(n.items[1].depth, 2);
    }
}
