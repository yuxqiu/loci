//! The body as a tree of blocks (§4.7), and block ids (§4.6): a `^id` ending a list item's
//! lead (nested items too) or a paragraph.

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

use super::*;

/// A block of the body, spanning its bytes, with the blocks inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// A heading block; it is a heading of the note only at the top level of the body
    /// ([`Markdown::headings`]).
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

    /// Whether `other`'s body has the same shape: blocks of the same kinds (every
    /// [`Block::Other`] is one kind, compared by the blocks inside it), nested the same
    /// way, headings of the same level, lists of as many items. Text, spans and block ids
    /// are not compared, so an edit that only adds or drops ids keeps the shape.
    pub fn same_shape(&self, other: &Markdown) -> bool {
        // The pairs of sibling blocks still to compare.
        let mut stack = vec![(self.body.as_slice(), other.body.as_slice())];
        while let Some((a, b)) = stack.pop() {
            if a.len() != b.len() {
                return false;
            }
            for (x, y) in a.iter().zip(b) {
                match (&x.node, &y.node) {
                    (Block::Heading(h), Block::Heading(k)) if h.level == k.level => {}
                    (Block::Paragraph { .. }, Block::Paragraph { .. }) => {}
                    (Block::List(xs), Block::List(ys)) if xs.len() == ys.len() => {
                        stack.extend(xs.iter().zip(ys).map(|(i, j)| (i.children.as_slice(), j.children.as_slice())));
                    }
                    (Block::Other(xs), Block::Other(ys)) => stack.push((xs, ys)),
                    _ => return false,
                }
            }
        }
        true
    }

    /// The document's headings, in order (§4.6): those at its top level. A heading block
    /// inside a block quote, a list item or a footnote is part of that block, not a
    /// heading of the document: no title, section or link target.
    pub fn headings(&self) -> impl Iterator<Item = Spanned<&Heading>> {
        self.body.iter().filter_map(|b| match &b.node {
            Block::Heading(h) => Some(Spanned::new(h, b.span.clone())),
            _ => None,
        })
    }

    /// Every heading with its section: from the heading up to the next heading of the
    /// same or a smaller level number, else the end of the text.
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

/// Whether `id` (without `^`) has the form of a block id (§4.6), as [`BLOCK_ID`] reads one.
pub(crate) fn is_block_id(id: &str) -> bool {
    static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\w+$").unwrap());
    ID.is_match(id)
}

/// Sets each block id (§4.6): the `^id` ending a paragraph or an item's lead. Nothing in
/// a block quote or a footnote has one (as in Obsidian, no part of a quotation does).
pub(super) fn assign_ids(text: &str, blocks: &mut [Spanned<Block>]) {
    for block in blocks {
        match &mut block.node {
            Block::Paragraph { id } => *id = trailing_id(text, &block.span),
            Block::List(items) => {
                for item in items {
                    item.node.id = trailing_id(text, &item.lead.span);
                    assign_ids(text, &mut item.node.children);
                }
            }
            Block::Heading(_) | Block::Other(_) => {}
        }
    }
}

/// The `^id` ending the text at `range`.
fn trailing_id(text: &str, range: &Range<usize>) -> Option<Spanned<BlockId>> {
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

    /// A tight item's text after a nested block is a paragraph, as in a loose item (§4.6).
    #[test]
    fn later_paragraph_in_a_tight_item() {
        let tight = "# T\n\n- item\n  ```\n  code\n  ```\n  more ^bbb\n- next ^ccc\n";
        assert_eq!(ids(&p(tight)), ["bbb", "ccc"]);
        let n = p(tight);
        let more = n.block_ids().next().unwrap();
        assert_eq!(&tight[more.span.clone()], "more ^bbb");
        let loose = "# T\n\n- item\n  ```\n  code\n  ```\n\n  more ^bbb\n";
        assert_eq!(ids(&p(loose)), ["bbb"]);
        assert_eq!(ids(&p("# T\n\n- a\n  - b\n  *more* text ^m\n  ***\n  after ^n\n")), ["m", "n"]);
        // A rule is a nested block: it ends the lead.
        assert_eq!(ids(&p("# T\n\n- item ^aaa\n  ***\n  after ^bbb\n")), ["aaa", "bbb"]);
    }

    /// A task box is part of the item's marker; an escaped first character is part of
    /// the lead, backslash included (§4.6).
    #[test]
    fn marker_and_lead_as_written() {
        let t = "# T\n\n- [ ] Q :: A ^aaa\n- \\# x\n";
        let n = p(t);
        let Block::List(items) = &n.body[1].node else { panic!("a list") };
        let parts: Vec<_> = items.iter().map(|i| (i.marker.value(t), i.lead.value(t))).collect();
        assert_eq!(parts, [("- [ ] ", "Q :: A ^aaa"), ("- ", "\\# x")]);
        assert_eq!(ids(&n), ["aaa"]);
        let h = p("# \\# x\n");
        assert_eq!(h.headings().next().unwrap().content.value(&h.text), "\\# x");
    }

    /// Nothing in a block quote or a footnote has a block id (§4.6, as in Obsidian).
    #[test]
    fn no_block_ids_in_quotes() {
        assert!(ids(&p("# T\n\n> quoted ^aaa\n>\n> - item ^bbb\n\n[^f]: note ^ccc\n")).is_empty());
    }

    /// The shape is the tree, not the order of kinds: a paragraph inside an item differs
    /// from one after the list.
    #[test]
    fn shape_compares_nesting() {
        let md = |t: &str| crate::parse::Markdown::parse(t.to_string());
        assert!(!md("- a\n\n  p\n").same_shape(&md("- a\n\np\n")));
        assert!(md("- a\n\n  p\n").same_shape(&md("- b ^x\n\n  q\n")));
        assert!(!md("# A\n").same_shape(&md("## A\n")), "a heading's level");
        assert!(!md("- a\n- b\n").same_shape(&md("- a\n")), "a list's items");
    }
}
