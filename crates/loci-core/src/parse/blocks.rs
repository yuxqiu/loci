//! The body as a tree of blocks (§4.7), and block ids (§4.6): a `^id` ending a list item's
//! lead (nested items too) or a paragraph.

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

use super::*;

/// A block of the body, spanning its bytes, with the blocks inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading(Heading),
    /// A paragraph, with the block id ending it. An item's leading paragraph is the
    /// item's lead, not a block.
    Paragraph {
        id: Option<Spanned<BlockId>>,
    },
    List(Vec<Spanned<ListItem>>),
    /// Any other block, with the blocks inside it: a block quote's or a footnote's;
    /// none for code, HTML, a table or a rule.
    Other(Vec<Spanned<Block>>),
}

/// A list item, spanning its bytes, nested blocks included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// The list marker; the item's content starts after it.
    pub marker: Spanned<Marker>,
    /// The item's leading text (§4.6): after the marker, up to its first nested block,
    /// without trailing whitespace.
    pub lead: Spanned<Lead>,
    /// The item's block id, ending its lead.
    pub id: Option<Spanned<BlockId>>,
    /// The blocks after its lead.
    pub children: Vec<Spanned<Block>>,
}

/// A block id (§4.6), spanning `^id`; its value is the id, without the `^`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockId;

impl Written for BlockId {
    fn value(bytes: &str) -> &str {
        &bytes[1..]
    }
}

/// A list item's marker (`-`, `1.`, with a task box `[ ]`) and the spaces after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker;

impl Written for Marker {}

/// A list item's leading text (§4.6), where its block id ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lead;

impl Written for Lead {}

/// A block or a list item, as [`Markdown::descendants`] visits them.
#[derive(Debug, Clone, Copy)]
pub enum Node<'a> {
    Block(&'a Spanned<Block>),
    Item(&'a Spanned<ListItem>),
}

/// The walk of [`Markdown::descendants`]: a stack of the lists of blocks or items being
/// visited, innermost last.
struct Descendants<'a> {
    stack: Vec<Frame<'a>>,
}

enum Frame<'a> {
    Blocks(std::slice::Iter<'a, Spanned<Block>>),
    Items(std::slice::Iter<'a, Spanned<ListItem>>),
}

impl<'a> Iterator for Descendants<'a> {
    type Item = Node<'a>;

    fn next(&mut self) -> Option<Node<'a>> {
        loop {
            let next = match self.stack.last_mut()? {
                Frame::Blocks(blocks) => blocks.next().map(Node::Block),
                Frame::Items(items) => items.next().map(Node::Item),
            };
            let Some(node) = next else {
                self.stack.pop();
                continue;
            };
            // What is inside the node comes next, before its siblings.
            match node {
                Node::Block(Spanned { node: Block::List(items), .. }) => self.stack.push(Frame::Items(items.iter())),
                Node::Block(Spanned { node: Block::Other(children), .. }) => {
                    self.stack.push(Frame::Blocks(children.iter()))
                }
                Node::Item(item) => self.stack.push(Frame::Blocks(item.children.iter())),
                Node::Block(_) => {}
            }
            return Some(node);
        }
    }
}

/// A block id and the bytes of the block it names (an item with its nested blocks, or
/// a paragraph).
#[derive(Debug, Clone, Copy)]
pub struct IdBlock<'a> {
    pub id: &'a Spanned<BlockId>,
    pub span: &'a Range<usize>,
}

impl Markdown {
    /// Every block and list item, depth first in document order (an item before the
    /// blocks inside it).
    pub fn descendants(&self) -> impl Iterator<Item = Node<'_>> {
        Descendants { stack: vec![Frame::Blocks(self.body.iter())] }
    }

    /// Every heading, nested ones included, in document order.
    pub fn headings(&self) -> impl Iterator<Item = Spanned<&Heading>> {
        let heading = |node| match node {
            Node::Block(Spanned { node: Block::Heading(h), span }) => Some(Spanned::new(h, span.clone())),
            _ => None,
        };
        self.descendants().filter_map(heading)
    }

    /// Every heading with its section: the heading through the next heading of the same
    /// or a higher level.
    pub fn sections(&self) -> impl Iterator<Item = (Spanned<&Heading>, Range<usize>)> {
        let headings: Vec<_> = self.headings().collect();
        (0..headings.len()).map(move |i| {
            let h = &headings[i];
            let end = headings[i + 1..].iter().find(|n| n.level <= h.level).map_or(self.text.len(), |n| n.span.start);
            (h.clone(), h.span.start..end)
        })
    }

    /// Every block id (§4.6), in document order.
    pub fn block_ids(&self) -> impl Iterator<Item = IdBlock<'_>> {
        let block_id = |node| match node {
            Node::Block(Spanned { node: Block::Paragraph { id: Some(id) }, span }) => Some(IdBlock { id, span }),
            Node::Item(item) => item.id.as_ref().map(|id| IdBlock { id, span: &item.span }),
            Node::Block(_) => None,
        };
        self.descendants().filter_map(block_id)
    }
}

/// A `^id` ending a text.
static BLOCK_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|\s)(\^\w+)\s*$").unwrap());

/// The `^id` ending the text at `range`.
pub(super) fn trailing_id(text: &str, range: &Range<usize>) -> Option<Spanned<BlockId>> {
    let id = BLOCK_ID.captures(&text[range.clone()])?.get(1)?;
    Some(Spanned::new(BlockId, range.start + id.start()..range.start + id.end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::testing::concept as p;

    fn ids(n: &Note) -> Vec<&str> {
        n.block_ids().map(|b| b.id.value(&n.text)).collect()
    }

    #[test]
    fn block_ids_tight_and_loose() {
        let t = "# T\n\n- one ^aaa\n- two\n  continued ^bbb\n\npara ^ccc\n";
        let n = p(t);
        assert_eq!(ids(&n), ["aaa", "bbb", "ccc"]);
        let para = n.block_ids().nth(2).unwrap();
        assert_eq!(&t[para.span.clone()], "para ^ccc\n", "a paragraph's block is the paragraph");
        assert_eq!(ids(&p("# T\n\n- one ^aaa\n\n- two ^bbb\n")), ["aaa", "bbb"]);
    }

    #[test]
    fn block_id_before_nested_list() {
        let t = "# T\n\n- parent ^ppp\n  - child ^ccc\n- next\n";
        let n = p(t);
        assert_eq!(ids(&n), ["ppp", "ccc"]);
        let Block::List(items) = &n.body[1].node else { panic!("a list") };
        assert_eq!(items[0].lead.value(t), "parent ^ppp");
        assert!(matches!(&items[0].children[..], [Spanned { node: Block::List(nested), .. }] if nested.len() == 1));
    }
}
