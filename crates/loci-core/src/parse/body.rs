//! The walk over the markdown events: headings, top-level blocks, list items, links,
//! counts and the metadata block, each with its byte range.

use std::ops::Range;
use std::sync::LazyLock;

use pulldown_cmark::{CodeBlockKind, Event, LinkType, MetadataBlockKind, Parser, Tag, TagEnd};
use regex::Regex;

use super::links::wikilink;
use super::*;

static URL_SCHEME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]*:").unwrap());

/// Walks the markdown after a BOM of `base` bytes. Ranges refer to the whole file.
pub(super) fn walk_body(text: &str, base: usize) -> Walker<'_> {
    let mut w = Walker { text, ..Walker::default() };
    for (event, range) in Parser::new_ext(&text[base..], options()).into_offset_iter() {
        let range = range.start + base..range.end + base;
        // Spans that only later events close: an item's content, link labels, heading
        // content.
        w.item_content(&event, &range);
        w.link_labels(&event, &range);
        w.heading_content(&event, &range);

        match event {
            Event::Start(tag) => w.start(tag, range),
            Event::End(tag_end) => {
                if matches!(tag_end, TagEnd::MetadataBlock(_)) {
                    w.in_metadata = false;
                }
                w.stack.pop();
            }
            // The YAML may arrive as several text events (e.g. with CRLF endings).
            Event::Text(_) if w.in_metadata => {
                w.meta_text = Some(w.meta_text.clone().map_or(range.clone(), |prev| prev.start..range.end));
            }
            Event::InlineMath(_) | Event::DisplayMath(_) => w.counts.math += 1,
            Event::Rule | Event::Html(_) if w.stack.is_empty() => {
                w.top_blocks.push(Spanned::new(BlockType::Other, range))
            }
            _ => {}
        }
    }

    w
}

/// What each open tag is, to know depth and item context.
enum Open {
    Item { idx: usize, saw_child: bool },
    List,
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
    /// The top-level paragraphs, where block ids may end (§4.6).
    pub(super) paragraphs: Vec<Range<usize>>,
    /// The YAML metadata block, `---` lines included, and the YAML text inside it.
    pub(super) meta_block: Option<Range<usize>>,
    pub(super) meta_text: Option<Range<usize>>,

    text: &'a str,
    stack: Vec<Open>,
    in_metadata: bool,
    /// The heading being read: its level, its bytes, and the extent of the events
    /// inside it so far.
    heading: Option<(u8, Range<usize>, Option<Range<usize>>)>,
    /// An item was just opened: its next event, unless it ends the item, starts its
    /// content (after the list marker).
    item_open: bool,
    /// Per open link or image, the markdown link it is (an index into `markdown_links`), if
    /// any, and the extent of the events inside it so far.
    links_open: Vec<(Option<usize>, Option<Range<usize>>)>,
}

impl Walker<'_> {
    fn item_content(&mut self, event: &Event, range: &Range<usize>) {
        if std::mem::take(&mut self.item_open) && !matches!(event, Event::End(_)) {
            let item = self.items.last_mut().expect("an item is open");
            item.node.marker.span.end = range.start;
        }
    }

    /// A markdown link's label is the extent of the events inside it.
    fn link_labels(&mut self, event: &Event, range: &Range<usize>) {
        if let Event::End(TagEnd::Link | TagEnd::Image) = event {
            let (markdown, label) = self.links_open.pop().expect("a link is open");
            if let (Some(i), Some(label)) = (markdown, label) {
                self.markdown_links[i].node.label.span = label;
            }
            return;
        }
        for (markdown, label) in &mut self.links_open {
            if markdown.is_none_or(|i| self.markdown_links[i].span.start <= range.start) {
                *label = Some(label.as_ref().map_or(range.clone(), |l| l.start..range.end));
            }
        }
    }

    /// A heading's content is the extent of the events inside it; at its end, its text
    /// and form follow from that.
    fn heading_content(&mut self, event: &Event, range: &Range<usize>) {
        let Some((level, span, content)) = &mut self.heading else { return };
        if let Event::End(TagEnd::Heading(_)) = event {
            let text = self.text;
            // No content (`#`): an empty range where the content would end.
            let end = span.start + text[span.clone()].trim_end().len();
            let content = content.take().unwrap_or(end..end);
            let setext = !text[span.start..content.start].trim_start_matches(' ').starts_with('#');
            let joined = text[content.clone()].lines().collect::<Vec<_>>().join("\n");
            let heading = Heading { level: *level, setext, content: Spanned::new(joined, content) };
            self.headings.push(Spanned::new(heading, span.clone()));
            self.heading = None;
        } else if span.start <= range.start {
            // pulldown-cmark reports a task-list marker inside the heading that follows it,
            // with the marker's own range, before the heading: only events within count.
            *content = Some(content.as_ref().map_or(range.clone(), |c| c.start..range.end));
        }
    }

    /// A tag opens: records the block, heading, item or link it starts.
    fn start(&mut self, tag: Tag, range: Range<usize>) {
        let top_level = self.stack.is_empty();
        self.end_item_lead(&tag, &range);
        let top_blocks = &mut self.top_blocks;
        let mut top = |block: BlockType| {
            if top_level {
                top_blocks.push(Spanned::new(block, range.clone()));
            }
        };

        let image = matches!(tag, Tag::Image { .. });
        let open = match tag {
            Tag::MetadataBlock(MetadataBlockKind::YamlStyle) => {
                self.in_metadata = true;
                self.meta_block = Some(range);
                Open::Other
            }
            Tag::Heading { level, .. } => {
                let level = level as u8;
                top(BlockType::Heading(level));
                self.heading = Some((level, range, None));
                Open::Other
            }
            Tag::Paragraph => {
                top(BlockType::Paragraph);
                self.paragraphs.push(range);
                Open::Other
            }
            Tag::List(_) => {
                top(BlockType::List);
                Open::List
            }
            Tag::Item => {
                let depth = self.stack.iter().filter(|o| matches!(o, Open::List)).count();
                // The marker's end is found from the item's first event ([`Walker::item_content`]),
                // the lead from its first block ([`Walker::end_item_lead`]).
                let (marker, lead) = (Spanned::new(Marker, range.clone()), Spanned::new(Lead, range.clone()));
                self.items.push(Spanned::new(ListItem { depth, marker, lead }, range));
                self.item_open = true;
                Open::Item { idx: self.items.len() - 1, saw_child: false }
            }
            Tag::CodeBlock(kind) => {
                top(BlockType::Other);
                self.counts.code += usize::from(matches!(kind, CodeBlockKind::Fenced(_)));
                Open::Other
            }
            Tag::Link { link_type, dest_url, .. } | Tag::Image { link_type, dest_url, .. } => {
                self.counts.images += usize::from(image);
                let markdown = handle_link(
                    &mut self.links,
                    &mut self.markdown_links,
                    self.text,
                    link_type,
                    &dest_url,
                    range,
                    image,
                );
                self.links_open.push((markdown, None));
                Open::Other
            }
            _ => {
                top(BlockType::Other);
                Open::Other
            }
        };

        self.stack.push(open);
    }

    /// A nested block inside an item ends that item's leading text (§4.6): a loose
    /// item's lead is its first paragraph; a tight item's, the text up to the block.
    fn end_item_lead(&mut self, tag: &Tag, range: &Range<usize>) {
        let is_block = matches!(
            tag,
            Tag::Paragraph
                | Tag::List(_)
                | Tag::CodeBlock(_)
                | Tag::BlockQuote(_)
                | Tag::HtmlBlock
                | Tag::Table(_)
                | Tag::Heading { .. }
                | Tag::FootnoteDefinition(_)
        );
        let Some(Open::Item { idx, saw_child }) = self.stack.last_mut().filter(|_| is_block) else { return };
        if std::mem::replace(saw_child, true) {
            return;
        }
        let item = &mut self.items[*idx];
        item.node.lead.span = if matches!(tag, Tag::Paragraph) { range.clone() } else { item.span.start..range.start };
    }
}

/// Records the link at `range`: a wikilink in `links`, a markdown link to a vault file
/// in `markdown_links`. Returns the index of a new markdown link.
fn handle_link(
    links: &mut Vec<Spanned<Link>>,
    markdown_links: &mut Vec<Spanned<MarkdownLink>>,
    text: &str,
    link_type: LinkType,
    dest: &str,
    range: Range<usize>,
    image: bool,
) -> Option<usize> {
    match link_type {
        LinkType::WikiLink { .. } => {
            let form = if image { LinkForm::Embed } else { LinkForm::Wiki };
            links.extend(wikilink(text, range, form));
            None
        }
        LinkType::Autolink | LinkType::Email => None,
        _ if URL_SCHEME.is_match(dest) => None,
        _ => {
            let label = Spanned::new(Label, range.end..range.end);
            markdown_links.push(Spanned::new(MarkdownLink { url: dest.to_string(), image, label }, range));
            Some(markdown_links.len() - 1)
        }
    }
}
