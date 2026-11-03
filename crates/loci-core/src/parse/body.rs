//! The walk over the markdown events: headings, top-level blocks, list items, links,
//! counts and the metadata block, each with its byte range.
//!
//! pulldown-cmark gives an element's whole range when it opens, but parts of it (a
//! heading's content, a link's label, where an item's marker ends) are known only from
//! the events inside it. So the walk keeps a stack of open elements, each with the
//! extent of the events inside it so far, and builds an element when it closes.

use std::ops::Range;
use std::sync::LazyLock;

use pulldown_cmark::{CodeBlockKind, CowStr, Event, LinkType, MetadataBlockKind, Parser, Tag};
use regex::Regex;

use super::blocks::trailing_id;
use super::links::wikilink;
use super::*;

static URL_SCHEME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]*:").unwrap());

/// Walks the markdown after a BOM of `base` bytes. Ranges refer to the whole file.
pub(super) fn walk_body(text: &str, base: usize) -> Walker<'_> {
    let mut w = Walker { text, ..Walker::default() };
    for (event, range) in Parser::new_ext(&text[base..], options()).into_offset_iter() {
        let range = range.start + base..range.end + base;
        match event {
            Event::Start(tag) => {
                w.extend_open(&range);
                w.open(tag, range);
            }
            Event::End(_) => w.close(),
            event => {
                w.extend_open(&range);
                match event {
                    Event::InlineMath(_) | Event::DisplayMath(_) => w.counts.math += 1,
                    Event::Rule | Event::Html(_) if w.stack.is_empty() => {
                        w.top_blocks.push(Spanned::new(BlockType::Other, range));
                    }
                    _ => {}
                }
            }
        }
    }

    // Elements are built as they close, so a nested one comes before its parent.
    w.items.sort_by_key(|i| i.span.start);
    w.markdown_links.sort_by_key(|m| m.span.start);
    w
}

/// A YAML metadata block: its bytes, `---` lines included, and the YAML text inside.
pub(super) struct Meta {
    pub(super) block: Range<usize>,
    pub(super) yaml: Range<usize>,
}

/// An element that has opened and not yet closed.
struct Open<'a> {
    kind: OpenKind<'a>,
    span: Range<usize>,
    /// The extent of the events inside it so far.
    inner: Option<Range<usize>>,
}

enum OpenKind<'a> {
    Metadata,
    Heading(u8),
    List,
    /// The item's leading text (§4.6), once its first nested block shows where it ends.
    Item {
        lead: Option<Range<usize>>,
    },
    Link {
        link_type: LinkType,
        dest: CowStr<'a>,
        image: bool,
    },
    Other,
}

/// A walk over the markdown events: what it collects, and its state between events.
#[derive(Default)]
pub(super) struct Walker<'a> {
    pub(super) headings: Vec<Spanned<Heading>>,
    pub(super) top_blocks: Vec<Spanned<BlockType>>,
    pub(super) items: Vec<Spanned<ListItem>>,
    pub(super) links: Vec<Spanned<Link>>,
    pub(super) markdown_links: Vec<Spanned<MarkdownLink>>,
    pub(super) counts: Counts,
    /// The paragraphs, where block ids may end (§4.6), but for an item's leading
    /// paragraph, whose id is the item's.
    pub(super) paragraphs: Vec<Range<usize>>,
    /// The YAML metadata block.
    pub(super) meta: Option<Meta>,

    text: &'a str,
    stack: Vec<Open<'a>>,
}

impl<'a> Walker<'a> {
    /// An event at `range` is inside every open element whose span holds it: the inner
    /// extent starts at the first such event and ends at the furthest. (pulldown-cmark
    /// reports a task box after the first event of its item, and inside the heading
    /// that follows it, with the box's own range: the box stays in the item's marker,
    /// and the span check keeps it out of the heading.)
    fn extend_open(&mut self, range: &Range<usize>) {
        for open in self.stack.iter_mut().filter(|o| o.span.start <= range.start && range.end <= o.span.end) {
            open.inner = Some(open.inner.as_ref().map_or(range.clone(), |i| i.start..i.end.max(range.end)));
        }
    }

    /// A tag opens: a top-level block is recorded, an item's lead may end, and the
    /// element goes on the stack.
    fn open(&mut self, tag: Tag<'a>, range: Range<usize>) {
        if let Some(block) = block_type(&tag).filter(|_| self.stack.is_empty()) {
            self.top_blocks.push(Spanned::new(block, range.clone()));
        }
        let lead = self.end_item_lead(&tag, &range);

        let kind = match tag {
            Tag::MetadataBlock(MetadataBlockKind::YamlStyle) => OpenKind::Metadata,
            Tag::Heading { level, .. } => OpenKind::Heading(level as u8),
            Tag::Paragraph => {
                if !lead {
                    self.paragraphs.push(range.clone());
                }
                OpenKind::Other
            }
            Tag::List(_) => OpenKind::List,
            Tag::Item => OpenKind::Item { lead: None },
            Tag::CodeBlock(kind) => {
                self.counts.code += usize::from(matches!(kind, CodeBlockKind::Fenced(_)));
                OpenKind::Other
            }
            Tag::Link { link_type, dest_url, .. } => OpenKind::Link { link_type, dest: dest_url, image: false },
            Tag::Image { link_type, dest_url, .. } => {
                self.counts.images += 1;
                OpenKind::Link { link_type, dest: dest_url, image: true }
            }
            _ => OpenKind::Other,
        };
        self.stack.push(Open { kind, span: range, inner: None });
    }

    /// The first nested block of an item ends the item's leading text (§4.6): a loose
    /// item's lead is that paragraph; a tight item's, its text after the marker up to
    /// the block. Returns whether the block is the lead.
    fn end_item_lead(&mut self, tag: &Tag, range: &Range<usize>) -> bool {
        let Some(Open { kind: OpenKind::Item { lead: lead @ None }, inner, .. }) = self.stack.last_mut() else {
            return false;
        };
        if !is_nested_block(tag) {
            return false;
        }
        let is_paragraph = matches!(tag, Tag::Paragraph);
        // The marker ends where the item's first event starts; this block is one.
        let marker_end = inner.as_ref().expect("the block is inside the item").start;
        *lead = Some(if is_paragraph { range.clone() } else { marker_end..range.start });
        is_paragraph
    }

    /// The innermost element closes: it is built from its span and the events inside.
    fn close(&mut self) {
        let Open { kind, span, inner } = self.stack.pop().expect("an element is open");
        match kind {
            OpenKind::Metadata => {
                // An empty block (`---` then `---`) holds an empty YAML text.
                let yaml = inner.unwrap_or(span.start..span.start);
                self.meta = Some(Meta { block: span, yaml });
            }
            OpenKind::Heading(level) => {
                let heading = heading(self.text, level, &span, inner);
                self.headings.push(Spanned::new(heading, span));
            }
            OpenKind::Item { lead } => {
                let depth = self.stack.iter().filter(|o| matches!(o.kind, OpenKind::List)).count();
                // The marker ends where the item's first event starts.
                let marker = Spanned::new(Marker, span.start..inner.map_or(span.end, |i| i.start));
                // No nested block: the lead is the rest of the item. Its trailing
                // whitespace (a line break, the next line's indent) is not its text.
                let lead = lead.unwrap_or(marker.span.end..span.end);
                let lead = Spanned::new(Lead, lead.start..lead.start + self.text[lead].trim_end().len());
                let id = trailing_id(self.text, &lead.span);
                self.items.push(Spanned::new(ListItem { depth, marker, lead, id }, span));
            }
            OpenKind::Link { link_type, dest, image } => {
                let label = inner.unwrap_or(span.end..span.end);
                self.link(link_type, &dest, span, label, image);
            }
            OpenKind::List | OpenKind::Other => {}
        }
    }

    /// Records the link at `span`: a wikilink in `links`, a markdown link to a vault
    /// file in `markdown_links` (its text at `label`).
    fn link(&mut self, link_type: LinkType, dest: &str, span: Range<usize>, label: Range<usize>, image: bool) {
        match link_type {
            LinkType::WikiLink { .. } => {
                let form = if image { LinkForm::Embed } else { LinkForm::Wiki };
                self.links.extend(wikilink(self.text, span, form));
            }
            LinkType::Autolink | LinkType::Email => {}
            _ if URL_SCHEME.is_match(dest) => {}
            _ => {
                let link = MarkdownLink { url: dest.to_string(), image, label: Spanned::new(Label, label) };
                self.markdown_links.push(Spanned::new(link, span));
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
    let joined = text[content.clone()].lines().collect::<Vec<_>>().join("\n");
    Heading { level, setext, content: Spanned::new(joined, content) }
}

/// What a tag is as a top-level block, if it is a block.
fn block_type(tag: &Tag) -> Option<BlockType> {
    match tag {
        Tag::Heading { level, .. } => Some(BlockType::Heading(*level as u8)),
        Tag::Paragraph => Some(BlockType::Paragraph),
        Tag::List(_) => Some(BlockType::List),
        Tag::MetadataBlock(MetadataBlockKind::YamlStyle) | Tag::Item | Tag::Link { .. } | Tag::Image { .. } => None,
        _ => Some(BlockType::Other),
    }
}

/// Whether a tag is a block that ends the leading text of the item holding it.
fn is_nested_block(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::List(_)
            | Tag::CodeBlock(_)
            | Tag::BlockQuote(_)
            | Tag::HtmlBlock
            | Tag::Table(_)
            | Tag::Heading { .. }
            | Tag::FootnoteDefinition(_)
    )
}
