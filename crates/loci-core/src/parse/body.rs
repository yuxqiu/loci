//! The walk over the body's markdown events: its blocks, links and counts, each with
//! its byte range.
//!
//! pulldown-cmark gives an element's whole range when it opens, but parts of it (a
//! heading's content, a link's label, where an item's marker ends) are known only from
//! the events inside it, and the blocks inside it only as they close. So the walk keeps
//! a stack of open elements, each with the extent of the events inside it so far and
//! the blocks closed inside it, and builds an element when it closes, into its parent.
//!
//! The walk reads normalized events ([`super::events`]); block ids are set on the
//! finished tree ([`super::blocks::assign_ids`]).

use std::ops::Range;
use std::sync::LazyLock;

use pulldown_cmark::{CodeBlockKind, CowStr, Event, LinkType, Parser, Tag};
use regex::Regex;

use super::blocks::assign_ids;
use super::events::normalize;
use super::links::wikilink;
use super::*;

static URL_SCHEME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]*:").unwrap());

/// Walks the body, from `base` to the end. Ranges refer to the whole file.
pub(super) fn walk_body(text: &str, base: usize) -> Walked {
    let mut w = Walker { text, stack: vec![], out: Walked::default() };
    let events = Parser::new_ext(&text[base..], options()).into_offset_iter();
    for (event, range) in normalize(text, events.map(|(e, r)| (e, r.start + base..r.end + base))) {
        match event {
            Event::Start(tag) => {
                w.extend_open(&range);
                w.open(tag, range);
            }
            Event::End(_) => w.close(),
            event => {
                w.extend_open(&range);
                match event {
                    Event::InlineMath(_) | Event::DisplayMath(_) => w.out.counts.math += 1,
                    Event::Rule => w.add(Spanned::new(Block::Other(vec![]), range)),
                    _ => {}
                }
            }
        }
    }

    // Markdown links are built as they close, so one in another's label comes first.
    w.out.markdown_links.sort_by_key(|m| m.span.start);
    assign_ids(text, &mut w.out.body);
    w.out
}

/// What the walk collects.
#[derive(Default)]
pub(super) struct Walked {
    pub(super) body: Vec<Spanned<Block>>,
    pub(super) links: Vec<Spanned<Link>>,
    pub(super) markdown_links: Vec<Spanned<MarkdownLink>>,
    pub(super) counts: Counts,
}

/// The walk's state between events.
struct Walker<'a> {
    text: &'a str,
    stack: Vec<Open<'a>>,
    out: Walked,
}

/// An element that has opened and not yet closed.
struct Open<'a> {
    kind: OpenKind<'a>,
    span: Range<usize>,
    /// The extent of the events inside it so far.
    inner: Option<Range<usize>>,
}

enum OpenKind<'a> {
    Heading(u8),
    Paragraph,
    List(Vec<Spanned<ListItem>>),
    /// The blocks closed inside the item.
    Item(Vec<Spanned<Block>>),
    /// Any other block, with the blocks closed inside it.
    Other(Vec<Spanned<Block>>),
    Link {
        link_type: LinkType,
        dest: CowStr<'a>,
        image: bool,
    },
    /// A table cell, which builds nothing.
    Cell,
    /// Anything else (inline markup, a table's head and rows), which builds nothing.
    Inline,
}

impl<'a> Walker<'a> {
    /// An event at `range` is inside every open element: the inner extent starts at the
    /// first such event and ends at the furthest.
    fn extend_open(&mut self, range: &Range<usize>) {
        for open in &mut self.stack {
            open.inner = Some(open.inner.as_ref().map_or(range.clone(), |i| i.start..i.end.max(range.end)));
        }
    }

    /// A tag opens: the element goes on the stack.
    fn open(&mut self, tag: Tag<'a>, range: Range<usize>) {
        let kind = match tag {
            Tag::Heading { level, .. } => OpenKind::Heading(level as u8),
            Tag::Paragraph => OpenKind::Paragraph,
            Tag::List(_) => OpenKind::List(vec![]),
            Tag::Item => OpenKind::Item(vec![]),
            Tag::CodeBlock(kind) => {
                self.out.counts.fenced_code += usize::from(matches!(kind, CodeBlockKind::Fenced(_)));
                OpenKind::Other(vec![])
            }
            Tag::BlockQuote(_) | Tag::HtmlBlock | Tag::Table(_) | Tag::FootnoteDefinition(_) => OpenKind::Other(vec![]),
            Tag::TableCell => OpenKind::Cell,
            Tag::Link { link_type, dest_url, .. } => OpenKind::Link { link_type, dest: dest_url, image: false },
            Tag::Image { link_type, dest_url, .. } => {
                self.out.counts.images += 1;
                OpenKind::Link { link_type, dest: dest_url, image: true }
            }
            _ => OpenKind::Inline,
        };
        self.stack.push(Open { kind, span: range, inner: None });
    }

    /// The innermost element closes: it is built from its span and the events inside,
    /// into its parent.
    fn close(&mut self) {
        let Open { kind, span, inner } = self.stack.pop().expect("an element is open");
        match kind {
            OpenKind::Heading(level) => {
                let heading = heading(self.text, level, &span, inner);
                self.add(Spanned::new(Block::Heading(heading), span));
            }
            OpenKind::Paragraph => self.add(Spanned::new(Block::Paragraph { id: None }, span)),
            OpenKind::List(items) => self.add(Spanned::new(Block::List(items), span)),
            OpenKind::Item(mut children) => {
                // The marker ends where the item's first event starts.
                let marker = Spanned::new(Marker, span.start..inner.map_or(span.end, |i| i.start));
                // The lead (§4.6) is the item's first block if it is a paragraph, else
                // empty after the marker; its trailing whitespace is not its text.
                let lead = match children.first() {
                    Some(Spanned { node: Block::Paragraph { .. }, .. }) => children.remove(0).span,
                    _ => marker.span.end..marker.span.end,
                };
                let lead = Spanned::new(Lead, lead.start..lead.start + self.text[lead].trim_end().len());
                let item = Spanned::new(ListItem { marker, lead, id: None, children }, span);
                match self.stack.last_mut() {
                    Some(Open { kind: OpenKind::List(items), .. }) => items.push(item),
                    _ => unreachable!("an item closes inside its list"),
                }
            }
            OpenKind::Other(children) => self.add(Spanned::new(Block::Other(children), span)),
            OpenKind::Link { link_type, dest, image } => {
                let label = inner.unwrap_or(span.end..span.end);
                self.link(link_type, &dest, span, label, image);
            }
            OpenKind::Cell | OpenKind::Inline => {}
        }
    }

    /// Adds a closed block to the innermost element that holds blocks, or to the body.
    fn add(&mut self, block: Spanned<Block>) {
        match self.stack.last_mut().map(|o| &mut o.kind) {
            None => self.out.body.push(block),
            Some(OpenKind::Item(children) | OpenKind::Other(children)) => children.push(block),
            Some(_) => unreachable!("blocks close inside the body, an item or another block"),
        }
    }

    /// Records the link at `span`: a wikilink in `links`, a markdown link to a vault
    /// file in `markdown_links` (its text at `label`).
    fn link(&mut self, link_type: LinkType, dest: &str, span: Range<usize>, label: Range<usize>, image: bool) {
        match link_type {
            LinkType::WikiLink { .. } => {
                let form = if image { LinkForm::Embed } else { LinkForm::Wiki };
                self.out.links.extend(wikilink(self.text, span, form));
            }
            LinkType::Autolink | LinkType::Email => {}
            _ if URL_SCHEME.is_match(dest) => {}
            _ => {
                let in_table = self.stack.iter().any(|o| matches!(o.kind, OpenKind::Cell));
                let link = MarkdownLink { url: dest.to_string(), image, label: Spanned::new(Label, label), in_table };
                self.out.markdown_links.push(Spanned::new(link, span));
            }
        }
    }
}

/// The heading of `level` at `span` whose inner events cover `inner` (§4.6).
fn heading(text: &str, level: u8, span: &Range<usize>, inner: Option<Range<usize>>) -> Heading {
    // No content (`#`): an empty range where the content would end.
    let end = span.start + text[span.clone()].trim_end().len();
    let content = inner.unwrap_or(end..end);
    let setext = !text[span.start..content.start].trim_start_matches(' ').starts_with('#');
    Heading { level, setext, content: Spanned::new(HeadingText, content) }
}
