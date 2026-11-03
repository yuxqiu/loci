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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockId {
    pub id: String,
    /// The bytes of `^id`.
    pub span: Range<usize>,
    pub block: Range<usize>,
    pub kind: BlockKind,
}

/// A `^id` ending a text: the marker (`^id`) and the id.
static BLOCK_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|\s)(\^(\w+))\s*$").unwrap());

/// Block ids (§4.6), in file order: an item's ends its lead; a paragraph's ends a
/// paragraph that is no item's lead.
pub(super) fn block_ids(text: &str, items: &[ListItem], paragraphs: &[Range<usize>]) -> Vec<BlockId> {
    let mut out = vec![];
    for item in items {
        if let Some((id, span)) = trailing_id(text, &item.lead) {
            out.push(BlockId { id, span, block: item.span.clone(), kind: BlockKind::Item });
        }
    }

    let leads: std::collections::HashSet<&Range<usize>> = items.iter().map(|i| &i.lead).collect();
    for p in paragraphs.iter().filter(|p| !leads.contains(p)) {
        if let Some((id, span)) = trailing_id(text, p) {
            out.push(BlockId { id, span, block: p.clone(), kind: BlockKind::Paragraph });
        }
    }

    out.sort_by_key(|b| b.span.start);
    out
}

/// The `^id` ending the text at `range`: the id, and the bytes of `^id`.
fn trailing_id(text: &str, range: &Range<usize>) -> Option<(String, Range<usize>)> {
    let caps = BLOCK_ID.captures(&text[range.clone()])?;
    let (marker, id) = (caps.get(1)?, caps.get(2)?);
    Some((id.as_str().to_string(), range.start + marker.start()..range.start + marker.end()))
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
