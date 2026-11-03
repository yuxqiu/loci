//! Parsing a markdown file into byte-spanned structure (m0 §4, §5.1).

use std::ops::Range;

use pulldown_cmark::Options;

use crate::id::Ulid;
use crate::path::Kind;
use crate::path::VaultPath;

mod blocks;
mod body;
mod frontmatter;
mod links;

use blocks::blocks;
use body::{Walker, walk_body};
use frontmatter::{frontmatter, frontmatter_links};

pub use blocks::{Block, BlockKind};
pub use frontmatter::{ConceptField, FmChange, FmEntry, FmValue, Frontmatter, new_concept_frontmatter, prereq_slug};
pub use links::{Fragment, Label, Link, LinkForm, MarkdownLink, Names, Target, heading_key};

pub const BOM: &str = "\u{feff}";

/// A value and the bytes of the note's text it was read from (after rustc's
/// `Spanned`). A part whose value is just its bytes is spanned with a marker type
/// naming it (a link's [`Label`]); [`Spanned::text`] reads the bytes.
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
    /// Raw heading content (§4.6): without the ATX `#` markers or a setext underline,
    /// lines joined with `\n`.
    pub content: Spanned<String>,
    /// A setext heading (text over a `===` or `---` underline), not an ATX one.
    pub setext: bool,
}

/// A list item, spanning its bytes, nested items included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// 1 for a top-level item.
    pub depth: usize,
    /// The list marker; the item's content starts after it.
    pub marker: Spanned<Marker>,
    /// The item's leading text (§4.6): after the marker, up to its first nested block,
    /// without trailing whitespace.
    pub lead: Spanned<Lead>,
    /// The item's block id, ending its lead, without its `^`; spanning `^id`.
    pub id: Option<Spanned<String>>,
}

/// A list item's marker (`-`, `1.`, with a task box `[ ]`) and the spaces after it: its
/// value is its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker;

/// A list item's leading text (§4.6), where its block id ends: its value is its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lead;

/// How much of each [`Material`] a note holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub math: usize,
    pub images: usize,
    pub code: usize,
}

impl Counts {
    pub fn of(&self, material: Material) -> usize {
        match material {
            Material::Math => self.math,
            Material::Images => self.images,
            Material::FencedCode => self.code,
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

    /// Named for reports.
    pub fn name(self) -> &'static str {
        match self {
            Material::Math => "math spans",
            Material::Images => "images",
            Material::FencedCode => "fenced code blocks",
        }
    }
}

/// What a block at the top level of the body is (§4.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    /// A heading of this level.
    Heading(u8),
    Paragraph,
    List,
    Other,
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
    /// The note's title (§9.3): the first H1's raw text, else the file stem.
    pub fn title(&self) -> &str {
        self.headings.iter().find(|h| h.level == 1).map_or_else(|| self.path.stem(), |h| h.content.as_str())
    }
}

/// What a markdown text holds (§4): the parse of a file, wherever it is.
#[derive(Debug, Clone)]
pub struct Markdown {
    pub text: String,
    pub frontmatter: Option<Spanned<Frontmatter>>,
    pub headings: Vec<Spanned<Heading>>,
    /// The top-level blocks after the frontmatter, in order.
    pub top_blocks: Vec<Spanned<BlockType>>,
    pub items: Vec<Spanned<ListItem>>,
    /// The blocks with a block id (§4.6), in order.
    pub blocks: Vec<Spanned<Block>>,
    /// The vault's links: wikilinks, embeds and `prereqs` items, in text order.
    pub links: Vec<Spanned<Link>>,
    /// Markdown-syntax links to vault files, which lint reports (`link.markdown`).
    pub markdown_links: Vec<Spanned<MarkdownLink>>,
    pub counts: Counts,
}

impl Markdown {
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

    /// The concept id: frontmatter `id`, if it is a ULID (lint reports one that is not).
    pub fn id(&self) -> Option<Ulid> {
        self.fm_str(ConceptField::Id.key())?.parse().ok()
    }

    /// The section under `self.headings[i]`: the heading through the next heading of the
    /// same or a higher level.
    pub fn section(&self, i: usize) -> Range<usize> {
        let h = &self.headings[i];
        let end = self.headings[i + 1..].iter().find(|n| n.level <= h.level).map_or(self.text.len(), |n| n.span.start);
        h.span.start..end
    }

    /// The bytes `fragment` names (§5.4): the section under the first heading it
    /// matches, or the block with its id.
    pub fn fragment_range(&self, fragment: &Fragment) -> Option<Range<usize>> {
        match fragment {
            Fragment::Heading(h) => {
                let key = heading_key(h);
                let i = self.headings.iter().position(|n| heading_key(&n.content) == key)?;
                Some(self.section(i))
            }
            Fragment::Block(id) => self.blocks.iter().find(|b| *b.id == *id).map(|b| b.span.clone()),
        }
    }

    /// Every H2 whose text is `text`, with the content of its section (after the
    /// heading, up to the next H1 or H2).
    pub fn h2_sections(&self, text: &str) -> Vec<(&Spanned<Heading>, Range<usize>)> {
        (0..self.headings.len())
            .filter(|&i| self.headings[i].level == 2 && *self.headings[i].content == text)
            .map(|i| (&self.headings[i], self.headings[i].span.end..self.section(i).end))
            .collect()
    }

    /// The top level of every H2 section whose text is `text`: its depth-1 list items,
    /// and its other top-level blocks.
    pub fn section_top_level(&self, text: &str) -> (Vec<&Spanned<ListItem>>, Vec<&Spanned<BlockType>>) {
        let sections: Vec<Range<usize>> = self.h2_sections(text).into_iter().map(|(_, r)| r).collect();
        let inside = |r: &Range<usize>| sections.iter().any(|s| s.start <= r.start && r.end <= s.end);
        let items = self.items.iter().filter(|i| i.depth == 1 && inside(&i.span)).collect();
        let blocks = self.top_blocks.iter().filter(|b| inside(&b.span)).collect();
        (items, blocks)
    }

    /// Whether the file starts with a UTF-8 byte-order mark (`file.bom` in managed notes).
    pub fn has_bom(&self) -> bool {
        self.text.starts_with(BOM)
    }

    /// Byte offset where the body starts: after the frontmatter block, or after a BOM.
    pub fn body_start(&self) -> usize {
        match &self.frontmatter {
            Some(f) if f.closed() => f.span.end,
            _ if self.has_bom() => BOM.len(),
            _ => 0,
        }
    }

    /// Whether the frontmatter is broken (unterminated or not a mapping), so its fields
    /// cannot be read; lint reports it (`fm.unterminated`, `fm.yaml`).
    pub fn frontmatter_broken(&self) -> bool {
        self.frontmatter.as_ref().is_some_and(|f| !matches!(f.node, Frontmatter::Fields { .. }))
    }
}

/// The markdown extensions loci reads notes with.
pub fn options() -> Options {
    Options::ENABLE_WIKILINKS
        | Options::ENABLE_MATH
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

/// Parses `text` (the whole file) as a note at `path`.
pub fn parse(path: &VaultPath, kind: Kind, text: String) -> Note {
    Note { path: path.clone(), kind, markdown: markdown(text) }
}

/// Parses `text`, a whole markdown file in the vault or not.
pub fn markdown(text: String) -> Markdown {
    // Frontmatter starts at byte 0, after a BOM (§4.3), which pulldown-cmark would read
    // as text: the walk starts past it.
    let base = if text.starts_with(BOM) { BOM.len() } else { 0 };
    let walk = walk_body(&text, base);
    let frontmatter = frontmatter(&text, base, &walk);
    let blocks = blocks(&text, &walk.items, &walk.paragraphs);
    let mut links = frontmatter.iter().flat_map(|f| frontmatter_links(f)).collect::<Vec<_>>();
    let Walker { headings, top_blocks, items, links: body_links, markdown_links, counts, .. } = walk;
    links.extend(body_links);
    links.sort_by_key(|l| l.span.start);
    Markdown { text, frontmatter, headings, top_blocks, items, blocks, links, markdown_links, counts }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Note {
        parse(&VaultPath::new("concepts/a/x.md".into()).unwrap(), Kind::Concept, text.to_string())
    }

    #[test]
    fn heading_texts() {
        let t = "# T\n\n## Hello world ##\n\n# C#\n\nTitle\nmore\n=====\n\n##\n";
        let n = p(t);
        let found: Vec<(&str, bool)> = n.headings.iter().map(|h| (h.content.as_str(), h.setext)).collect();
        assert_eq!(found, [("T", false), ("Hello world", false), ("C#", false), ("Title\nmore", true), ("", false)]);
        assert_eq!(n.headings[1].content.text(t), "Hello world");
        // Found by the parse-span property: a task marker is reported inside the heading.
        let n = p("- [ ] ---\n  - \n");
        assert_eq!(*n.headings[0].content, "---");
    }

    #[test]
    fn bom_offsets() {
        let t = format!("{BOM}# T\n\n[[a]]\n");
        let n = parse(&VaultPath::new("views/v.md".into()).unwrap(), Kind::View, t.clone());
        assert!(n.has_bom());
        assert_eq!(&t[n.links[0].target.span.clone()], "a");
    }
}
