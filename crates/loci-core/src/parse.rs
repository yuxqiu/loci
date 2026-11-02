//! Parsing a markdown file into byte-spanned structure (m0 §4, §5.1).

use std::ops::Range;
use std::sync::LazyLock;

use pulldown_cmark::{CodeBlockKind, Event, LinkType, MetadataBlockKind, Options, Parser, Tag, TagEnd};
use regex::Regex;
use saphyr::{LoadableYamlNode, MarkedYaml, Scalar, YamlData};

use crate::id::Ulid;
use crate::path::VaultPath;
use crate::vault::Kind;

pub const BOM: &str = "\u{feff}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FmValue {
    Str {
        value: String,
        span: Range<usize>,
        quoted: bool,
    },
    List {
        items: Vec<FmValue>,
        span: Range<usize>,
        flow: bool,
    },
    /// Any other YAML value (number, bool, null, mapping).
    Other {
        span: Range<usize>,
    },
}

impl FmValue {
    pub fn span(&self) -> Range<usize> {
        match self {
            FmValue::Str { span, .. } | FmValue::List { span, .. } | FmValue::Other { span } => span.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FmEntry {
    pub key: String,
    pub key_span: Range<usize>,
    pub value: FmValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FmError {
    Unterminated,
    Yaml(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmatter {
    /// The whole block, `---` lines included (byte range in the file).
    pub range: Range<usize>,
    pub entries: Vec<FmEntry>,
}

impl Frontmatter {
    pub fn get(&self, key: &str) -> Option<&FmEntry> {
        self.entries.iter().find(|e| e.key == key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    /// Raw heading content (§4.6): the bytes at `content`, lines joined with `\n`.
    pub text: String,
    pub range: Range<usize>,
    /// The content's byte range: without the ATX `#` markers or a setext underline.
    pub content: Range<usize>,
    /// A setext heading (text over a `===` or `---` underline), not an ATX one.
    pub setext: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub range: Range<usize>,
    /// 1 for a top-level item.
    pub depth: usize,
    /// The item's leading text (§4.6).
    pub lead: Range<usize>,
    /// Where the item's content starts, after its list marker.
    pub content_start: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Item,
    Paragraph,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockId {
    pub id: String,
    /// Range of `^id`.
    pub range: Range<usize>,
    pub block: Range<usize>,
    pub kind: BlockKind,
}

/// Serialized in lowercase (`wiki`, `embed`, `prereq`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkForm {
    Wiki,
    Embed,
    Prereq,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub form: LinkForm,
    pub range: Range<usize>,
    pub target_range: Range<usize>,
    /// Raw target text (the bytes at `target_range`; a table's escaping `\` before the
    /// `|` is outside it).
    pub raw: String,
    /// Normalised target (§5.2), without the fragment.
    pub target: String,
    pub fragment: Option<Fragment>,
    /// The byte range of a wikilink's `|label`, without the `|`.
    pub label: Option<Range<usize>>,
}

/// A link's `#fragment` (§5.4): a heading, or a block id (`#^id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fragment {
    Heading(String),
    /// The id, without its `^`.
    Block(String),
}

impl Fragment {
    fn parse(s: &str) -> Self {
        match s.strip_prefix('^') {
            Some(id) => Self::Block(id.to_string()),
            None => Self::Heading(s.to_string()),
        }
    }
}

/// As written after the `#`.
impl std::fmt::Display for Fragment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Heading(h) => f.write_str(h),
            Self::Block(id) => write!(f, "^{id}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdLink {
    pub range: Range<usize>,
    pub url: String,
    pub image: bool,
    /// The bytes of the link text (an image's alt text); empty when there is none.
    pub label: Range<usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub math: usize,
    pub images: usize,
    pub code: usize,
}

impl Counts {
    /// The kinds of material counted, named for reports, in the order of [`Counts::values`].
    pub const NAMES: [&'static str; 3] = ["math spans", "images", "fenced code blocks"];

    pub fn values(&self) -> [usize; 3] {
        [self.math, self.images, self.code]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopBlock {
    Heading(u8),
    Paragraph,
    List,
    Other,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub path: VaultPath,
    pub kind: Kind,
    pub text: String,
    pub bom: bool,
    pub frontmatter: Option<Frontmatter>,
    pub fm_error: Option<FmError>,
    pub headings: Vec<Heading>,
    /// Top-level blocks after the frontmatter, in order, with their ranges.
    pub top_blocks: Vec<(TopBlock, Range<usize>)>,
    pub items: Vec<ListItem>,
    pub block_ids: Vec<BlockId>,
    pub links: Vec<Link>,
    pub md_links: Vec<MdLink>,
    pub counts: Counts,
}

impl Note {
    /// The value of frontmatter field `key`.
    pub fn fm(&self, key: &str) -> Option<&FmValue> {
        Some(&self.frontmatter.as_ref()?.get(key)?.value)
    }

    /// Frontmatter field `key`, if it is a string.
    pub fn fm_str(&self, key: &str) -> Option<&str> {
        match self.fm(key)? {
            FmValue::Str { value, .. } => Some(value),
            _ => None,
        }
    }

    /// The concept id: frontmatter `id`, if it is a ULID (lint reports one that is not).
    pub fn id(&self) -> Option<Ulid> {
        self.fm_str("id")?.parse().ok()
    }

    /// The note's title (§9.3): the first H1's raw text, else the file stem.
    pub fn title(&self) -> &str {
        self.headings.iter().find(|h| h.level == 1).map_or_else(|| self.path.stem(), |h| h.text.as_str())
    }

    /// The section under `self.headings[i]`: the heading through the next heading of the
    /// same or a higher level.
    pub fn section(&self, i: usize) -> Range<usize> {
        let h = &self.headings[i];
        let end = self.headings[i + 1..].iter().find(|n| n.level <= h.level).map_or(self.text.len(), |n| n.range.start);
        h.range.start..end
    }

    /// Every H2 whose text is `text`, with the content of its section (after the
    /// heading, up to the next H1 or H2).
    pub fn h2_sections(&self, text: &str) -> Vec<(&Heading, Range<usize>)> {
        (0..self.headings.len())
            .filter(|&i| self.headings[i].level == 2 && self.headings[i].text == text)
            .map(|i| (&self.headings[i], self.headings[i].range.end..self.section(i).end))
            .collect()
    }

    /// The top level of every H2 section whose text is `text`: its depth-1 list items,
    /// and its other top-level blocks.
    pub fn section_top_level(&self, text: &str) -> (Vec<&ListItem>, Vec<&(TopBlock, Range<usize>)>) {
        let sections: Vec<Range<usize>> = self.h2_sections(text).into_iter().map(|(_, r)| r).collect();
        let inside = |r: &Range<usize>| sections.iter().any(|s| s.start <= r.start && r.end <= s.end);
        let items = self.items.iter().filter(|i| i.depth == 1 && inside(&i.range)).collect();
        let blocks = self.top_blocks.iter().filter(|(_, r)| inside(r)).collect();
        (items, blocks)
    }

    /// Byte offset where the body starts: after the frontmatter block, or after a BOM.
    pub fn body_start(&self) -> usize {
        match &self.frontmatter {
            Some(f) => f.range.end,
            None if self.bom => BOM.len(),
            None => 0,
        }
    }
}

static BLOCK_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|\s)\^(\w+)\s*$").unwrap());
static URL_SCHEME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]*:").unwrap());

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

/// The slug of a `prereqs` item, if it is a quoted `"[[<slug>]]"` written verbatim
/// (m0 §6.1), and its bytes in `text`. Only such an item is a link.
pub fn prereq_item<'a>(text: &str, item: &'a FmValue) -> Option<(&'a str, Range<usize>)> {
    let FmValue::Str { value, span, quoted: true } = item else { return None };
    // Escapes and folded lines would make offsets into the value wrong for the file.
    let raw = &text[span.clone()];
    if raw.len() < 2 || &raw[1..raw.len() - 1] != value {
        return None;
    }
    let inner = value.strip_prefix("[[")?.strip_suffix("]]")?;
    inner.parse::<crate::id::Slug>().ok()?;
    let start = span.start + "\"[[".len();
    Some((inner, start..start + inner.len()))
}

/// Normalises a link target (§5.2): returns (target, fragment), or `None` for `[[]]`.
fn normalize_target(raw: &str) -> Option<(String, Option<Fragment>)> {
    let s = raw.trim_matches(|c: char| c.is_ascii_whitespace());
    let (t, frag) = match s.find('#') {
        Some(i) => (&s[..i], Some(Fragment::parse(&s[i + 1..]))),
        None => (s, None),
    };
    let t = t.strip_suffix(".md").unwrap_or(t);
    let t = crate::vault::nfc(t);
    if t.is_empty() && frag.is_none() {
        return None;
    }
    Some((t, frag))
}

/// Parses `text` (the whole file) as a note at `path`.
pub fn parse(path: &VaultPath, kind: Kind, text: String) -> Note {
    let bom = text.starts_with(BOM);
    let base = if bom { BOM.len() } else { 0 };
    let body = walk_body(&text, base);
    let (frontmatter, fm_error) = frontmatter(&text, base, &body);
    let block_ids = block_ids(&text, &body.items, &body.paragraphs);
    let mut links = body.links;
    if let Some(fm) = &frontmatter {
        links.extend(frontmatter_links(&text, fm));
    }
    links.sort_by_key(|l| l.range.start);
    Note {
        path: path.clone(),
        kind,
        text,
        bom,
        frontmatter,
        fm_error,
        headings: body.headings,
        top_blocks: body.top_blocks,
        items: body.items,
        block_ids,
        links,
        md_links: body.md_links,
        counts: body.counts,
    }
}

/// What one walk over the markdown events collects.
#[derive(Default)]
struct Body {
    headings: Vec<Heading>,
    top_blocks: Vec<(TopBlock, Range<usize>)>,
    items: Vec<ListItem>,
    paragraphs: Vec<Range<usize>>,
    links: Vec<Link>,
    md_links: Vec<MdLink>,
    counts: Counts,
    /// The YAML metadata block, `---` lines included.
    meta_block: Option<Range<usize>>,
    /// The YAML text inside the metadata block.
    meta_text: Option<Range<usize>>,
}

/// Walks the markdown after a BOM of `base` bytes. Ranges refer to the whole file.
fn walk_body(text: &str, base: usize) -> Body {
    // Container stack: what each open tag is, to know depth and item context.
    enum Open {
        Item { idx: usize, saw_child: bool },
        List,
        Other,
    }
    let mut b = Body::default();
    let mut stack: Vec<Open> = vec![];
    let mut in_metadata = false;
    // The heading being read, and the extent of the events inside it so far.
    let mut heading: Option<Option<Range<usize>>> = None;
    // An item was just opened: its next event, unless it ends the item, starts its content.
    let mut item_open = false;
    // Per open link or image, the markdown link it is (an index into `md_links`), if
    // any, and the extent of the events inside it so far.
    let mut links_open: Vec<(Option<usize>, Option<Range<usize>>)> = vec![];
    for (event, range) in Parser::new_ext(&text[base..], options()).into_offset_iter() {
        let range = range.start + base..range.end + base;
        if std::mem::take(&mut item_open) && !matches!(event, Event::End(_)) {
            b.items.last_mut().expect("an item is open").content_start = range.start;
        }
        if let Event::End(TagEnd::Link | TagEnd::Image) = event {
            let (md, label) = links_open.pop().expect("a link is open");
            if let (Some(i), Some(label)) = (md, label) {
                b.md_links[i].label = label;
            }
        } else {
            for (md, label) in &mut links_open {
                let within = md.is_none_or(|i| b.md_links[i].range.start <= range.start);
                if within {
                    *label = Some(label.as_ref().map_or(range.clone(), |l| l.start..range.end));
                }
            }
        }
        match (&mut heading, &event) {
            (Some(content), Event::End(TagEnd::Heading(_))) => {
                let h = b.headings.last_mut().expect("a heading is open");
                // No content (`#`): an empty range where the content would end.
                let end = h.range.start + text[h.range.clone()].trim_end().len();
                h.content = content.take().unwrap_or(end..end);
                h.text = text[h.content.clone()].lines().collect::<Vec<_>>().join("\n");
                h.setext = !text[h.range.start..h.content.start].trim_start_matches(' ').starts_with('#');
                heading = None;
            }
            // pulldown-cmark reports a task-list marker inside the heading that follows it,
            // with the marker's own range, before the heading: only events within count.
            (Some(content), _) if b.headings.last().is_some_and(|h| h.range.start <= range.start) => {
                *content = Some(content.as_ref().map_or(range.clone(), |c| c.start..range.end));
            }
            _ => {}
        }
        match event {
            Event::Start(tag) => {
                let top_level = stack.is_empty();
                // A nested block inside an item ends that item's leading text.
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
                if is_block && let Some(Open::Item { idx, saw_child }) = stack.last_mut() {
                    let item = &mut b.items[*idx];
                    if !*saw_child {
                        *saw_child = true;
                        item.lead = if matches!(tag, Tag::Paragraph) {
                            // Loose item: the lead is its first paragraph.
                            range.clone()
                        } else {
                            // Tight item: the text up to the first nested block.
                            item.range.start..range.start
                        };
                    }
                }
                let mut top = |block: TopBlock| {
                    if top_level {
                        b.top_blocks.push((block, range.clone()));
                    }
                };
                let image = matches!(tag, Tag::Image { .. });
                let open = match tag {
                    Tag::MetadataBlock(MetadataBlockKind::YamlStyle) => {
                        in_metadata = true;
                        b.meta_block = Some(range.clone());
                        Open::Other
                    }
                    Tag::Heading { level, .. } => {
                        let level = level as u8;
                        top(TopBlock::Heading(level));
                        b.headings.push(Heading {
                            level,
                            text: String::new(),
                            range: range.clone(),
                            content: range,
                            setext: false,
                        });
                        heading = Some(None);
                        Open::Other
                    }
                    Tag::Paragraph => {
                        top(TopBlock::Paragraph);
                        b.paragraphs.push(range);
                        Open::Other
                    }
                    Tag::List(_) => {
                        top(TopBlock::List);
                        Open::List
                    }
                    Tag::Item => {
                        let depth = stack.iter().filter(|o| matches!(o, Open::List)).count();
                        // The marker's end is found from the item's first event (below).
                        b.items.push(ListItem {
                            range: range.clone(),
                            depth,
                            lead: range.clone(),
                            content_start: range.end,
                        });
                        item_open = true;
                        Open::Item { idx: b.items.len() - 1, saw_child: false }
                    }
                    Tag::CodeBlock(kind) => {
                        top(TopBlock::Other);
                        if matches!(kind, CodeBlockKind::Fenced(_)) {
                            b.counts.code += 1;
                        }
                        Open::Other
                    }
                    Tag::Link { link_type, dest_url, .. } | Tag::Image { link_type, dest_url, .. } => {
                        b.counts.images += usize::from(image);
                        let md = handle_link(&mut b, text, link_type, &dest_url, range, image);
                        links_open.push((md, None));
                        Open::Other
                    }
                    _ => {
                        top(TopBlock::Other);
                        Open::Other
                    }
                };
                stack.push(open);
            }
            Event::End(tag_end) => {
                if matches!(tag_end, TagEnd::MetadataBlock(_)) {
                    in_metadata = false;
                }
                stack.pop();
            }
            Event::Text(_) if in_metadata => {
                // The YAML may arrive as several text events (e.g. with CRLF endings).
                b.meta_text = Some(match b.meta_text {
                    Some(prev) => prev.start..range.end,
                    None => range,
                });
            }
            Event::InlineMath(_) | Event::DisplayMath(_) => b.counts.math += 1,
            Event::Rule | Event::Html(_) if stack.is_empty() => {
                b.top_blocks.push((TopBlock::Other, range));
            }
            _ => {}
        }
    }
    b
}

/// The frontmatter (§4.3): a YAML metadata block at the start of the file.
fn frontmatter(text: &str, base: usize, body: &Body) -> (Option<Frontmatter>, Option<FmError>) {
    let Some(block) = body.meta_block.clone().filter(|r| r.start == base) else {
        let rest = &text[base..];
        let unterminated = rest.starts_with("---\n") || rest.starts_with("---\r\n") || rest == "---";
        return (None, unterminated.then_some(FmError::Unterminated));
    };
    let yaml_range = body.meta_text.clone().unwrap_or(block.start..block.start);
    match parse_yaml(&text[yaml_range.clone()], yaml_range.start) {
        Ok(entries) => (Some(Frontmatter { range: block, entries }), None),
        Err(e) => (Some(Frontmatter { range: block, entries: vec![] }), Some(FmError::Yaml(e))),
    }
}

/// Block ids (§4.6), in file order. A paragraph that is an item's lead carries the
/// item's id.
fn block_ids(text: &str, items: &[ListItem], paragraphs: &[Range<usize>]) -> Vec<BlockId> {
    let mut out = vec![];
    for item in items {
        if let Some(b) = block_id(text, item.lead.clone()) {
            out.push(BlockId { block: item.range.clone(), kind: BlockKind::Item, ..b });
        }
    }
    let leads: std::collections::HashSet<&Range<usize>> = items.iter().map(|i| &i.lead).collect();
    for p in paragraphs.iter().filter(|p| !leads.contains(p)) {
        if let Some(b) = block_id(text, p.clone()) {
            out.push(BlockId { kind: BlockKind::Paragraph, ..b });
        }
    }
    out.sort_by_key(|b| b.range.start);
    out
}

/// The `prereqs` links of a frontmatter (§5.1), from verbatim scalars only.
fn frontmatter_links(text: &str, fm: &Frontmatter) -> Vec<Link> {
    let mut out = vec![];
    let items = match fm.get("prereqs") {
        Some(FmEntry { value: FmValue::List { items, .. }, .. }) => items.as_slice(),
        _ => &[],
    };
    for item in items {
        if let Some((inner, target_range)) = prereq_item(text, item)
            && let Some((target, fragment)) = normalize_target(inner)
        {
            out.push(Link {
                form: LinkForm::Prereq,
                range: item.span(),
                target_range,
                raw: inner.to_string(),
                target,
                fragment,
                label: None,
            });
        }
    }
    out
}

/// Records the link at `range`: a wikilink in `links`, a markdown link to a vault file
/// in `md_links`. Returns the index of a new markdown link.
fn handle_link(
    b: &mut Body,
    text: &str,
    link_type: LinkType,
    dest: &str,
    range: Range<usize>,
    image: bool,
) -> Option<usize> {
    match link_type {
        LinkType::WikiLink { .. } => {
            let open = if image { "![[" } else { "[[" };
            let start = range.start + open.len();
            let slice = &text[start..range.end];
            let close = slice.find("]]").expect("pulldown-cmark reports a wikilink with its closing `]]`");
            let pipe = slice[..close].find('|');
            let label = pipe.map(|p| start + p + 1..start + close);
            // In a table the `|` is escaped (`\|`); the `\` is table syntax, not target.
            let target = &slice[..pipe.unwrap_or(close)];
            let target = target.strip_suffix('\\').filter(|_| pipe.is_some()).unwrap_or(target);
            let tr = start..start + target.len();
            if let Some((normalized, fragment)) = normalize_target(target) {
                b.links.push(Link {
                    form: if image { LinkForm::Embed } else { LinkForm::Wiki },
                    range,
                    target_range: tr,
                    raw: target.to_string(),
                    target: normalized,
                    fragment,
                    label,
                });
            }
            None
        }
        LinkType::Autolink | LinkType::Email => None,
        _ if URL_SCHEME.is_match(dest) => None,
        _ => {
            b.md_links.push(MdLink { label: range.end..range.end, range, url: dest.to_string(), image });
            Some(b.md_links.len() - 1)
        }
    }
}

fn block_id(text: &str, range: Range<usize>) -> Option<BlockId> {
    let slice = &text[range.clone()];
    let trimmed = slice.trim_end();
    let caps = BLOCK_ID.captures(trimmed)?;
    let m = caps.get(1).unwrap();
    let start = range.start + m.start() - 1; // include the `^`
    Some(BlockId {
        id: m.as_str().to_string(),
        range: start..range.start + m.end(),
        block: range,
        kind: BlockKind::Paragraph,
    })
}

/// Parses frontmatter YAML into top-level entries with byte spans relative to the file
/// (`offset` is the byte offset of `yaml` in the file).
fn parse_yaml(yaml: &str, offset: usize) -> Result<Vec<FmEntry>, String> {
    if yaml.trim().is_empty() {
        return Ok(vec![]);
    }
    // `info()` is the message without the position (§7.1: messages carry no positions).
    let docs = MarkedYaml::load_from_str(yaml).map_err(|e| e.info().to_string())?;
    let Some(doc) = docs.into_iter().next() else { return Ok(vec![]) };
    let YamlData::Mapping(map) = &doc.data else {
        return Err("the top level is not a mapping".into());
    };
    let yaml = Yaml::new(yaml, offset);
    let mut entries = vec![];
    for (k, v) in map.iter() {
        let Some(key) = string(k) else { return Err("a key is not a string".into()) };
        entries.push(FmEntry { key: key.to_string(), key_span: yaml.span(k), value: yaml.value(v) });
    }
    Ok(entries)
}

/// A YAML node's string, if it is a string scalar.
fn string<'a>(n: &'a MarkedYaml) -> Option<&'a str> {
    match &n.data {
        YamlData::Value(Scalar::String(s)) | YamlData::Representation(s, _, _) => Some(s),
        _ => None,
    }
}

/// YAML text at byte `offset` in the file, for converting saphyr's character
/// positions to file byte offsets (§4.4).
struct Yaml<'a> {
    text: &'a str,
    offset: usize,
    /// Character index -> byte index in `text`.
    bytes: Vec<usize>,
}

impl<'a> Yaml<'a> {
    fn new(text: &'a str, offset: usize) -> Self {
        let mut bytes: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
        bytes.push(text.len());
        Yaml { text, offset, bytes }
    }

    fn span(&self, n: &MarkedYaml) -> Range<usize> {
        let byte = |ci: usize| self.offset + self.bytes[ci];
        byte(n.span.start.index())..byte(n.span.end.index())
    }

    fn value(&self, n: &MarkedYaml) -> FmValue {
        let span = self.span(n);
        let source = &self.text[span.start - self.offset..];
        if let Some(s) = string(n) {
            let quoted = source.starts_with(['"', '\'']);
            return FmValue::Str { value: s.to_string(), span, quoted };
        }
        match &n.data {
            YamlData::Sequence(items) => {
                let flow = source.starts_with('[');
                FmValue::List { items: items.iter().map(|i| self.value(i)).collect(), span, flow }
            }
            _ => FmValue::Other { span },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Note {
        parse(&VaultPath::new("concepts/a/x.md".into()).unwrap(), Kind::Concept, text.to_string())
    }

    #[test]
    fn normalize() {
        assert_eq!(normalize_target(" a.md#H "), Some(("a".into(), Some(Fragment::Heading("H".into())))));
        assert_eq!(normalize_target("a#^b1"), Some(("a".into(), Some(Fragment::Block("b1".into())))));
        assert_eq!(normalize_target(""), None);
        assert_eq!(normalize_target("#h"), Some(("".into(), Some(Fragment::Heading("h".into())))));
    }

    #[test]
    fn heading_texts() {
        let t = "# T\n\n## Hello world ##\n\n# C#\n\nTitle\nmore\n=====\n\n##\n";
        let n = p(t);
        let found: Vec<(&str, bool)> = n.headings.iter().map(|h| (h.text.as_str(), h.setext)).collect();
        assert_eq!(found, [("T", false), ("Hello world", false), ("C#", false), ("Title\nmore", true), ("", false)]);
        assert_eq!(&t[n.headings[1].content.clone()], "Hello world");
        // Found by the parse-span property: a task marker is reported inside the heading.
        let n = p("- [ ] ---\n  - \n");
        assert_eq!(n.headings[0].text, "---");
    }

    #[test]
    fn frontmatter_spans() {
        let t = "---\nid: 01ABC\nprereqs: [\"[[a]]\", \"[[二分]]\"]\ndigest: \"é x\"\n---\n# T\n";
        let n = p(t);
        let fm = n.frontmatter.as_ref().unwrap();
        let FmValue::Str { span, quoted, .. } = &fm.get("digest").unwrap().value else { panic!() };
        assert!(quoted);
        assert_eq!(&t[span.clone()], "\"é x\"");
        let prereqs: Vec<_> = n.links.iter().filter(|l| l.form == LinkForm::Prereq).collect();
        assert_eq!(prereqs.len(), 2);
        assert_eq!(&t[prereqs[1].target_range.clone()], "二分");
    }

    #[test]
    fn unquoted_prereq_is_nested_list() {
        let n = p("---\nprereqs: [[a]]\n---\n# T\n");
        let fm = n.frontmatter.unwrap();
        let FmValue::List { items, .. } = &fm.get("prereqs").unwrap().value else { panic!() };
        assert!(matches!(items[0], FmValue::List { .. }));
    }

    #[test]
    fn duplicate_keys_rejected() {
        let n = p("---\nid: a\nid: b\n---\n# T\n");
        assert!(matches!(n.fm_error, Some(FmError::Yaml(_))));
    }

    #[test]
    fn unterminated() {
        let n = p("---\nid: a\n# T\n");
        assert_eq!(n.fm_error, Some(FmError::Unterminated));
    }

    #[test]
    fn wikilinks() {
        let t = "# T\n\nSee [[b#H|label]] and ![[x/y.png|300]] and [m](a/b.md) and [w](https://x).\n";
        let n = p(t);
        assert_eq!(n.links.len(), 2);
        assert_eq!(&t[n.links[0].target_range.clone()], "b#H");
        assert_eq!(n.links[0].target, "b");
        assert_eq!(n.links[0].fragment, Some(Fragment::Heading("H".into())));
        assert_eq!(n.links[1].form, LinkForm::Embed);
        assert_eq!(n.links[1].target, "x/y.png");
        let labels: Vec<Option<&str>> = n.links.iter().map(|l| l.label.clone().map(|r| &t[r])).collect();
        assert_eq!(labels, [Some("label"), Some("300")]);
        assert_eq!(n.md_links.len(), 1);
    }

    #[test]
    fn a_table_escape_is_outside_the_target() {
        let t = "# T\n\n| a | [[b\\|label]] |\n|---|---|\n";
        let n = p(t);
        let l = &n.links[0];
        assert_eq!((l.raw.as_str(), l.target.as_str()), ("b", "b"));
        assert_eq!(l.label.clone().map(|r| &t[r]), Some("label"));
    }

    #[test]
    fn no_links_in_code_or_math() {
        let n = p("# T\n\n`[[a]]` and $[[b]]$\n\n```\n[[c]]\n```\n\nprice $5 and $6\n");
        assert!(n.links.is_empty());
        assert_eq!(n.counts.code, 1);
    }

    #[test]
    fn table_pipe() {
        let t = "# T\n\n| a | b |\n|---|---|\n| [[x\\|y]] | z |\n";
        let n = p(t);
        assert_eq!(n.links.len(), 1);
        assert_eq!(n.links[0].target, "x");
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

    #[test]
    fn bom_offsets() {
        let t = format!("{BOM}# T\n\n[[a]]\n");
        let n = parse(&VaultPath::new("views/v.md".into()).unwrap(), Kind::View, t.clone());
        assert!(n.bom);
        assert_eq!(&t[n.links[0].target_range.clone()], "a");
    }
}
