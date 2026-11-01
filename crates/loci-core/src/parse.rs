//! Parsing a markdown file into byte-spanned structure (m0 §4, §5.1).

use std::ops::Range;
use std::sync::LazyLock;

use pulldown_cmark::{CodeBlockKind, Event, LinkType, MetadataBlockKind, Options, Parser, Tag, TagEnd};
use regex::Regex;
use saphyr::{LoadableYamlNode, MarkedYaml, Scalar, YamlData};
use unicode_normalization::UnicodeNormalization;

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
    /// The YAML text between the fences.
    pub yaml_range: Range<usize>,
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
    /// Raw heading content (§4.6).
    pub text: String,
    pub range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub range: Range<usize>,
    /// 1 for a top-level item.
    pub depth: usize,
    /// The item's leading text (§4.6).
    pub lead: Range<usize>,
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

/// Serialized in lowercase (`wiki`, `embed`, `prereq`, `source`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkForm {
    Wiki,
    Embed,
    Prereq,
    Source,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub form: LinkForm,
    pub range: Range<usize>,
    pub target_range: Range<usize>,
    /// Raw target text (the bytes at `target_range`).
    pub raw: String,
    /// Normalised target (§5.2), without the fragment.
    pub target: String,
    pub fragment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdLink {
    pub range: Range<usize>,
    pub url: String,
    pub image: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub math: usize,
    pub images: usize,
    pub code: usize,
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
    pub path: String,
    pub kind: Kind,
    pub text: String,
    pub bom: bool,
    pub frontmatter: Option<Frontmatter>,
    pub fm_error: Option<FmError>,
    pub headings: Vec<Heading>,
    /// Top-level blocks after the frontmatter, in order, with their ranges.
    pub top_blocks: Vec<(TopBlock, Range<usize>)>,
    pub items: Vec<ListItem>,
    pub paragraphs: Vec<Range<usize>>,
    pub block_ids: Vec<BlockId>,
    pub links: Vec<Link>,
    pub md_links: Vec<MdLink>,
    pub counts: Counts,
}

impl Note {
    /// Frontmatter field `key`, if it is a string.
    /// The value of frontmatter field `key`.
    pub fn fm(&self, key: &str) -> Option<&FmValue> {
        Some(&self.frontmatter.as_ref()?.get(key)?.value)
    }

    pub fn fm_str(&self, key: &str) -> Option<&str> {
        match self.fm(key)? {
            FmValue::Str { value, .. } => Some(value),
            _ => None,
        }
    }

    /// The string items of frontmatter list `key` (other items are skipped; lint
    /// reports them).
    pub fn fm_strings(&self, key: &str) -> Vec<&str> {
        match self.fm(key) {
            Some(FmValue::List { items, .. }) => items
                .iter()
                .filter_map(|i| match i {
                    FmValue::Str { value, .. } => Some(value.as_str()),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        }
    }

    /// The raw text of the first H1, if any.
    pub fn h1(&self) -> Option<&str> {
        self.headings.iter().find(|h| h.level == 1).map(|h| h.text.as_str())
    }

    /// The note's title (§9.3): the first H1's raw text, else the file stem.
    pub fn title(&self) -> &str {
        self.h1().unwrap_or_else(|| crate::vault::stem(&self.path))
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

fn options() -> Options {
    Options::ENABLE_WIKILINKS
        | Options::ENABLE_MATH
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

/// The slug of a `prereqs` value, if it is exactly `[[<slug>]]` (m0 §6.1; slugs match
/// case-insensitively).
pub fn prereq_slug(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("[[")?.strip_suffix("]]")?;
    crate::vault::is_slug(&inner.to_lowercase()).then_some(inner)
}

/// List item text without its marker (`-`, `*`, `+`, `1.` or `1)`) and leading spaces.
pub fn strip_list_marker(s: &str) -> &str {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix(['-', '*', '+']) {
        return rest.trim_start();
    }
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    match s[digits..].strip_prefix(['.', ')']) {
        Some(rest) if digits > 0 => rest.trim_start(),
        _ => s,
    }
}

/// True if a frontmatter scalar's source bytes are its value (inside its quotes when
/// quoted), so that offsets into the value are offsets into the file. Escapes and
/// folded lines break this.
pub fn verbatim(text: &str, span: &Range<usize>, value: &str, quoted: bool) -> bool {
    let raw = &text[span.clone()];
    if quoted { raw.len() >= 2 && &raw[1..raw.len() - 1] == value } else { raw == value }
}

/// Normalises a link target (§5.2): returns (target, fragment), or `None` for `[[]]`.
pub fn normalize_target(raw: &str) -> Option<(String, Option<String>)> {
    let s = raw.strip_suffix('\\').unwrap_or(raw);
    let s = s.trim_matches(|c: char| c.is_ascii_whitespace());
    let (t, frag) = match s.find('#') {
        Some(i) => (&s[..i], Some(s[i + 1..].to_string())),
        None => (s, None),
    };
    let t = t.strip_suffix(".md").unwrap_or(t);
    let t: String = t.nfc().collect();
    if t.is_empty() && frag.is_none() {
        return None;
    }
    Some((t, frag))
}

/// Parses `text` (the whole file) as a note at `path`.
pub fn parse(path: &str, kind: Kind, text: String) -> Note {
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
        path: path.to_string(),
        kind,
        text,
        bom,
        frontmatter,
        fm_error,
        headings: body.headings,
        top_blocks: body.top_blocks,
        items: body.items,
        paragraphs: body.paragraphs,
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
    for (event, range) in Parser::new_ext(&text[base..], options()).into_offset_iter() {
        let range = range.start + base..range.end + base;
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
                let open = match tag {
                    Tag::MetadataBlock(MetadataBlockKind::YamlStyle) => {
                        in_metadata = true;
                        b.meta_block = Some(range.clone());
                        Open::Other
                    }
                    Tag::Heading { level, .. } => {
                        let level = level as u8;
                        top(TopBlock::Heading(level));
                        b.headings.push(Heading { level, text: heading_text(&text[range.clone()]), range });
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
                        b.items.push(ListItem { range: range.clone(), depth, lead: range });
                        Open::Item { idx: b.items.len() - 1, saw_child: false }
                    }
                    Tag::CodeBlock(kind) => {
                        top(TopBlock::Other);
                        if matches!(kind, CodeBlockKind::Fenced(_)) {
                            b.counts.code += 1;
                        }
                        Open::Other
                    }
                    Tag::Link { link_type, dest_url, .. } => {
                        handle_link(&mut b, text, link_type, &dest_url, range, false);
                        Open::Other
                    }
                    Tag::Image { link_type, dest_url, .. } => {
                        b.counts.images += 1;
                        handle_link(&mut b, text, link_type, &dest_url, range, true);
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
        Ok(entries) => (Some(Frontmatter { range: block, yaml_range, entries }), None),
        Err(e) => (Some(Frontmatter { range: block, yaml_range, entries: vec![] }), Some(FmError::Yaml(e))),
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

/// The `prereqs` and `sources` links of a frontmatter (§5.1), from verbatim scalars only.
fn frontmatter_links(text: &str, fm: &Frontmatter) -> Vec<Link> {
    let mut out = vec![];
    let items = |key: &str| match fm.get(key) {
        Some(FmEntry { value: FmValue::List { items, .. }, .. }) => items.as_slice(),
        _ => &[],
    };
    for item in items("prereqs") {
        if let FmValue::Str { value, span, quoted: true } = item
            && verbatim(text, span, value, true)
            && let Some(inner) = prereq_slug(value)
            && let Some((target, fragment)) = normalize_target(inner)
        {
            // After the opening `"[[`.
            let start = span.start + 3;
            out.push(Link {
                form: LinkForm::Prereq,
                range: span.clone(),
                target_range: start..start + inner.len(),
                raw: inner.to_string(),
                target,
                fragment,
            });
        }
    }
    for item in items("sources") {
        if let FmValue::Str { value, span, quoted } = item
            && verbatim(text, span, value, *quoted)
            && let Some((target, fragment)) = normalize_target(value)
        {
            let q = usize::from(*quoted);
            out.push(Link {
                form: LinkForm::Source,
                range: span.clone(),
                target_range: span.start + q..span.end - q,
                raw: value.clone(),
                target,
                fragment,
            });
        }
    }
    out
}

fn handle_link(b: &mut Body, text: &str, link_type: LinkType, dest: &str, range: Range<usize>, image: bool) {
    match link_type {
        LinkType::WikiLink { .. } => {
            let open = if image { 3 } else { 2 };
            let start = range.start + open;
            let slice = &text[start..range.end];
            let close = slice.find("]]").unwrap_or(slice.len());
            let len = slice[..close].find('|').unwrap_or(close);
            let tr = start..start + len;
            let raw = &text[tr.clone()];
            if let Some((target, fragment)) = normalize_target(raw) {
                b.links.push(Link {
                    form: if image { LinkForm::Embed } else { LinkForm::Wiki },
                    range,
                    target_range: tr,
                    raw: raw.to_string(),
                    target,
                    fragment,
                });
            }
        }
        LinkType::Autolink | LinkType::Email => {}
        _ => {
            if !URL_SCHEME.is_match(dest) {
                b.md_links.push(MdLink { range, url: dest.to_string(), image });
            }
        }
    }
}

/// Heading text (§4.6): raw content, ATX markers and closing `#`s removed, setext
/// underline excluded, trimmed.
fn heading_text(raw: &str) -> String {
    let raw = raw.trim_end_matches(['\n', '\r']);
    if raw.trim_start().starts_with('#') {
        let s = raw.trim_start().trim_start_matches('#');
        let s = s.trim();
        // A closing sequence must be preceded by a space.
        let trimmed = s.trim_end_matches('#');
        let s = if trimmed.len() != s.len() && (trimmed.is_empty() || trimmed.ends_with(' ')) { trimmed } else { s };
        s.trim().to_string()
    } else {
        // Setext: everything but the underline.
        let mut lines: Vec<&str> = raw.lines().collect();
        lines.pop();
        lines.join("\n").trim().to_string()
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
        let byte = |ci: usize| self.offset + self.bytes[ci.min(self.bytes.len() - 1)];
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
        parse("concepts/a/x.md", Kind::Concept, text.to_string())
    }

    #[test]
    fn normalize() {
        assert_eq!(normalize_target(" a.md#H "), Some(("a".into(), Some("H".into()))));
        assert_eq!(normalize_target("x\\"), Some(("x".into(), None)));
        assert_eq!(normalize_target(""), None);
        assert_eq!(normalize_target("#h"), Some(("".into(), Some("h".into()))));
    }

    #[test]
    fn heading_texts() {
        assert_eq!(heading_text("## Hello world ##\n"), "Hello world");
        assert_eq!(heading_text("# C#\n"), "C#");
        assert_eq!(heading_text("Title\n=====\n"), "Title");
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
        assert_eq!(n.links[0].fragment.as_deref(), Some("H"));
        assert_eq!(n.links[1].form, LinkForm::Embed);
        assert_eq!(n.links[1].target, "x/y.png");
        assert_eq!(n.md_links.len(), 1);
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
        let n = parse("views/v.md", Kind::View, t.clone());
        assert!(n.bom);
        assert_eq!(&t[n.links[0].target_range.clone()], "a");
    }
}
