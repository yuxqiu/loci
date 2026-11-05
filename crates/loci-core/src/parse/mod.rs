//! Parsing a markdown file into byte-spanned structure (m0 §4, §5.1).

use std::borrow::Cow;
use std::ops::Range;

use pulldown_cmark::Options;

use crate::envelope::Refusal;
use crate::id::Ulid;
use crate::path::Kind;
use crate::path::VaultPath;

mod blocks;
mod body;
mod events;
mod frontmatter;
mod links;

use body::walk_body;
use frontmatter::{frontmatter, frontmatter_links, prereq_slug};

pub use blocks::{Block, BlockId, IdBlock, Lead, ListItem, Marker, Node};
pub use frontmatter::{ConceptField, FmChange, FmEntry, FmValue, Frontmatter, new_concept_frontmatter};
pub use links::{Fragment, Label, Link, LinkForm, MarkdownLink, Names, Target, atx_closing};
pub(crate) use links::{bare_slug, heading_key, line_ends, one_line};

pub(crate) const BOM: &str = "\u{feff}";

/// A value and the bytes of the note's text it was read from (after rustc's
/// `Spanned`). A part whose value is read from its bytes is spanned with a marker type
/// naming it (a link's [`Label`]), which implements [`Written`]; [`Spanned::text`] reads
/// the bytes of any part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Range<usize>,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Range<usize>) -> Self {
        Self { node, span }
    }

    /// The bytes of `src`, the note's text, as written.
    pub fn text<'t>(&self, src: &'t str) -> &'t str {
        &src[self.span.clone()]
    }
}

/// A part whose value is read from its bytes as written (§4.7): all of them by default,
/// a block id's without its `^`. A marker type implements it. A value that needs more
/// than its bytes (decoded YAML, a normalized link name) is stored instead.
pub trait Written {
    /// The value of a part written as `bytes`.
    fn value(bytes: &str) -> &str {
        bytes
    }
}

impl<T: Written> Spanned<T> {
    /// The part's value, read from `src`, the note's text.
    pub fn value<'t>(&self, src: &'t str) -> &'t str {
        T::value(self.text(src))
    }
}

impl<T> std::ops::Deref for Spanned<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.node
    }
}

/// A heading, spanning its bytes, `#` markers or setext underline included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    /// The heading's content as written; [`Heading::heading_text`] is its §4.6 heading text.
    pub content: Spanned<HeadingText>,
    /// A setext heading (text over a `===` or `---` underline), not an ATX one.
    pub setext: bool,
}

impl Heading {
    /// Its §4.6 heading text: its content read on one line, from `text`, the note it is in.
    pub fn heading_text<'t>(&self, text: &'t str) -> Cow<'t, str> {
        one_line(self.content.value(text))
    }
}

/// A heading's content as written, line endings kept, without the ATX `#` markers and
/// closing `#`s or a setext underline; §4.6's heading text is it read on one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingText;

impl Written for HeadingText {}

/// How much of each [`Material`] a note holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub math: usize,
    pub images: usize,
    pub fenced_code: usize,
}

impl std::ops::Index<Material> for Counts {
    type Output = usize;

    fn index(&self, material: Material) -> &usize {
        match material {
            Material::Math => &self.math,
            Material::Images => &self.images,
            Material::FencedCode => &self.fenced_code,
        }
    }
}

/// Material whose loss lint reports when notes are deleted (`material-dropped`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Material {
    Math,
    Images,
    FencedCode,
}

impl Material {
    pub const ALL: [Material; 3] = [Material::Math, Material::Images, Material::FencedCode];
}

/// Named for reports.
impl std::fmt::Display for Material {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Material::Math => "math spans",
            Material::Images => "images",
            Material::FencedCode => "fenced code blocks",
        })
    }
}

/// A vault note: its place in the vault, and what its text holds.
#[derive(Debug, Clone)]
pub struct Note {
    pub path: VaultPath,
    pub kind: Kind,
    pub markdown: Markdown,
}

impl std::ops::Deref for Note {
    type Target = Markdown;

    fn deref(&self) -> &Markdown {
        &self.markdown
    }
}

impl Note {
    /// Parses `text` (the whole file) as the note at `path`, a path core reads.
    pub fn parse(path: &VaultPath, text: String) -> Note {
        let kind = path.kind().expect("a note is at a path core reads");
        Note { path: path.clone(), kind, markdown: Markdown::parse(text) }
    }

    /// The note's title (§4.7): the first H1's heading text, on one line, else the file
    /// stem.
    pub fn title(&self) -> Cow<'_, str> {
        self.headings().find(|h| h.level == 1).map_or_else(|| self.path.stem().into(), |h| h.heading_text(&self.text))
    }

    /// This note, when its kind is one a command takes (`accepts`); else `note.kind`,
    /// saying it is not `what` (m0 §8).
    pub fn of_kind(&self, accepts: impl Fn(Kind) -> bool, what: &str) -> Result<&Self, Refusal> {
        if !accepts(self.kind) {
            return Err(Refusal::refused("note.kind", format!("`{}` is not {what}", self.path)));
        }
        Ok(self)
    }
}

/// What a markdown text holds (§4): the parse of a file, wherever it is.
#[derive(Debug, Clone)]
pub struct Markdown {
    pub text: String,
    pub frontmatter: Option<Spanned<Frontmatter>>,
    /// The top-level blocks after the frontmatter, in order, each holding its own.
    pub body: Vec<Spanned<Block>>,
    /// The vault's links: wikilinks, embeds and `prereqs` items, in text order.
    pub links: Vec<Spanned<Link>>,
    /// Markdown-syntax links to vault files, which lint reports (`link.markdown`).
    pub markdown_links: Vec<Spanned<MarkdownLink>>,
    pub counts: Counts,
}

impl Markdown {
    /// Parses `text`, a whole markdown file in the vault or not.
    pub fn parse(text: String) -> Markdown {
        // Frontmatter starts at byte 0, after a BOM (§4.3).
        let frontmatter = frontmatter(&text, if text.starts_with(BOM) { BOM.len() } else { 0 });
        let walked = walk_body(&text, body_start(&text, frontmatter.as_ref()));
        let mut links = frontmatter.iter().flat_map(|f| frontmatter_links(f)).collect::<Vec<_>>();
        links.extend(walked.links);
        links.sort_by_key(|l| l.span.start);
        Markdown {
            text,
            frontmatter,
            body: walked.body,
            links,
            markdown_links: walked.markdown_links,
            counts: walked.counts,
        }
    }

    /// The value of frontmatter field `key`.
    pub fn fm(&self, key: &str) -> Option<&Spanned<FmValue>> {
        Some(&self.frontmatter.as_ref()?.get(key)?.value)
    }

    /// Frontmatter field `key`, if it is a string.
    pub fn fm_str(&self, key: &str) -> Option<&str> {
        match &self.fm(key)?.node {
            FmValue::Str(value) => Some(value),
            _ => None,
        }
    }

    /// What in `prereqs` is no prereq link (§6.1): the items that are not, or the whole
    /// value when it is no list. An item is a link exactly when the parser read one
    /// from it.
    pub fn bad_prereqs(&self) -> Vec<&Spanned<FmValue>> {
        match self.fm(ConceptField::Prereqs.key()) {
            None => vec![],
            Some(Spanned { node: FmValue::List(items), .. }) => {
                items.iter().filter(|i| !matches!(&i.node, FmValue::Str(item) if prereq_slug(item).is_some())).collect()
            }
            Some(value) => vec![value],
        }
    }

    /// The concept id: frontmatter `id`, if it is a ULID (lint reports one that is not).
    pub fn id(&self) -> Option<Ulid> {
        self.fm_str(ConceptField::Id.key())?.parse().ok()
    }

    /// The bytes `fragment` names (§5.4): the section under the first heading it
    /// matches, or the block with its id.
    pub fn fragment_range(&self, fragment: &Fragment) -> Option<Range<usize>> {
        match fragment {
            Fragment::Heading(h) => {
                let key = heading_key(h);
                let text = &self.text;
                self.sections().find(|(n, _)| heading_key(n.content.value(text)) == key).map(|(_, section)| section)
            }
            Fragment::Block(id) => self.block_ids().find(|b| b.id.value(&self.text) == id).map(|b| b.span.clone()),
        }
    }

    /// Every H2 whose text is `text`, with the top-level blocks of its section.
    pub fn h2_sections(&self, text: &str) -> Vec<H2Section<'_>> {
        self.sections()
            .filter(|(h, _)| h.level == 2 && h.heading_text(&self.text) == text)
            .map(|(heading, section)| {
                let inside = |b: &&Spanned<Block>| heading.span.end <= b.span.start && b.span.end <= section.end;
                H2Section { blocks: self.body.iter().filter(inside).collect(), heading }
            })
            .collect()
    }

    /// Whether the file starts with a UTF-8 byte-order mark (`file.bom` in managed notes).
    pub fn has_bom(&self) -> bool {
        self.text.starts_with(BOM)
    }

    /// Byte offset where the body starts: after the frontmatter block, or after a BOM.
    pub fn body_start(&self) -> usize {
        body_start(&self.text, self.frontmatter.as_ref())
    }

    /// Whether the frontmatter is broken (unterminated or not a mapping), so its fields
    /// cannot be read; lint reports it (`fm.unterminated`, `fm.yaml`).
    pub fn has_broken_frontmatter(&self) -> bool {
        self.frontmatter.as_ref().is_some_and(|f| !matches!(f.node, Frontmatter::Fields { .. }))
    }
}

/// An H2 and the top-level blocks of its section (after the heading, up to the next H1
/// or H2), as [`Markdown::h2_sections`] finds them.
#[derive(Debug, Clone)]
pub struct H2Section<'a> {
    pub heading: Spanned<&'a Heading>,
    pub blocks: Vec<&'a Spanned<Block>>,
}

/// The markdown extensions loci reads notes with.
pub fn options() -> Options {
    Options::ENABLE_WIKILINKS
        | Options::ENABLE_MATH
        | Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

/// Byte offset where the body of `text` starts: after a closed frontmatter block, or
/// after a BOM.
fn body_start(text: &str, frontmatter: Option<&Spanned<Frontmatter>>) -> usize {
    match frontmatter {
        Some(f) if f.is_closed() => f.span.end,
        _ if text.starts_with(BOM) => BOM.len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::vp;

    use crate::testing::concept as p;

    #[test]
    fn heading_texts() {
        let t = "# T\n\n## Hello world ##\n\n# C#\n\nTitle\nmore\n=====\n\n##\n";
        let n = p(t);
        let headings: Vec<_> = n.headings().collect();
        let found: Vec<(&str, bool)> = headings.iter().map(|h| (h.content.value(t), h.setext)).collect();
        assert_eq!(found, [("T", false), ("Hello world", false), ("C#", false), ("Title\nmore", true), ("", false)]);
        // Found by the parse-span property: a task marker is reported inside the heading.
        let n = p("- [ ] ---\n  - \n");
        let nested = n.descendants().find_map(|b| match b {
            Node::Block(Spanned { node: Block::Heading(h), .. }) => Some(h.content.value(&n.text)),
            _ => None,
        });
        assert_eq!(nested, Some("---"));
        assert_eq!(n.headings().count(), 0, "a heading in an item is no heading of the note");
        let n = p("Title\r\nmore\r\n===\r\n");
        assert_eq!(n.headings().next().unwrap().content.value(&n.text), "Title\r\nmore", "the text as written (§4.6)");
    }

    /// The title and `h2_sections` read a setext heading on one line (§4.6).
    #[test]
    fn multi_line_title_and_section() {
        let n = p("Title\nmore\n===\n\nSec\nline\n---\n");
        assert_eq!(n.title(), "Title more");
        assert_eq!(n.h2_sections("Sec line").len(), 1);
    }

    #[test]
    fn bom_offsets() {
        let t = format!("{BOM}# T\n\n[[a]]\n");
        let n = Note::parse(&vp("views/v.md"), t.clone());
        assert!(n.has_bom());
        assert_eq!(&t[n.links[0].target.span.clone()], "a");
    }
}
