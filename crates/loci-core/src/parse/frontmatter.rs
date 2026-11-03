//! The frontmatter (m0 §4.3, §6.1): the YAML metadata block, read with the byte span of
//! every key and value, the `prereqs` links in it, and rewritten whole when loci edits it.

use std::collections::BTreeMap;
use std::ops::Range;

use saphyr::{LoadableYamlNode, Mapping, MarkedYaml, Scalar, Yaml, YamlData, YamlEmitter};
use serde::{Deserialize, Serialize};

use super::body::Meta;
use super::links::prereq;
use super::*;
use crate::id::{Slug, Ulid};
use crate::splice::Edit;

/// A frontmatter value, spanned in its entry; a string's span includes its quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FmValue {
    Str(String),
    List(Vec<Spanned<FmValue>>),
    /// Any other YAML value (number, bool, null, mapping).
    Other,
}

impl FmValue {
    /// Whether this `prereqs` item is an unquoted `[[slug]]`, which YAML reads as a list
    /// holding a list holding the slug (the `Yaml` form is `quote_prereqs`'s).
    pub fn is_unquoted_prereq(&self) -> bool {
        let FmValue::List(items) = self else { return false };
        let [Spanned { node: FmValue::List(items), .. }] = items.as_slice() else { return false };
        matches!(items.as_slice(), [Spanned { node: FmValue::Str(value), .. }] if value.parse::<Slug>().is_ok())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FmEntry {
    pub key: Spanned<String>,
    pub value: Spanned<FmValue>,
}

/// A note's frontmatter (§4.3): what the YAML block at its start is. Spans the block,
/// `---` lines and their breaks included; an unterminated one, its opening line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frontmatter {
    /// Opened with a `---` line that is never closed.
    Unterminated,
    /// A closed block that is not a YAML mapping, and why.
    Invalid(String),
    /// A YAML mapping: its fields, and the YAML text they were read from.
    Fields { entries: Vec<FmEntry>, yaml: Range<usize> },
}

impl Frontmatter {
    /// Whether the block is closed (its span is the whole block).
    pub fn is_closed(&self) -> bool {
        !matches!(self, Frontmatter::Unterminated)
    }

    /// The fields: none unless the block is a YAML mapping.
    pub fn entries(&self) -> &[FmEntry] {
        match self {
            Frontmatter::Fields { entries, .. } => entries,
            Frontmatter::Unterminated | Frontmatter::Invalid(_) => &[],
        }
    }

    pub fn get(&self, key: &str) -> Option<&FmEntry> {
        self.entries().iter().find(|e| *e.key == key)
    }
}

/// The frontmatter (§4.3): a YAML metadata block at the start of the file.
pub(super) fn frontmatter(text: &str, base: usize, meta: Option<&Meta>) -> Option<Spanned<Frontmatter>> {
    let Some(Meta { block: span, yaml }) = meta.filter(|m| m.block.start == base) else {
        let rest = &text[base..];
        let opening =
            ["---\n", "---\r\n"].into_iter().find(|o| rest.starts_with(o)).or((rest == "---").then_some("---"));
        return opening.map(|o| Spanned::new(Frontmatter::Unterminated, base..base + o.len()));
    };
    // The closing `---` line's break too, so the body starts after it.
    let rest = &text[span.end..];
    let line_break = ["\r\n", "\n"].into_iter().find(|b| rest.starts_with(b)).map_or(0, str::len);
    let span = span.start..span.end + line_break;

    let frontmatter = match parse_yaml(&text[yaml.clone()], yaml.start) {
        Ok(entries) => Frontmatter::Fields { entries, yaml: yaml.clone() },
        Err(reason) => Frontmatter::Invalid(reason),
    };
    Some(Spanned::new(frontmatter, span))
}

/// The `prereqs` links (§6.1): each string item `[[<slug>]]`. Any other item is no link,
/// and lint reports it (`fm.field-type`).
pub(super) fn frontmatter_links(fm: &Frontmatter) -> Vec<Spanned<Link>> {
    let items = match fm.get(ConceptField::Prereqs.key()).map(|e| &e.value.node) {
        Some(FmValue::List(items)) => items.as_slice(),
        _ => &[],
    };
    let slug = |item: &Spanned<FmValue>| match &item.node {
        FmValue::Str(value) => Some(prereq(item.span.clone(), prereq_slug(value)?)),
        _ => None,
    };
    items.iter().filter_map(slug).collect()
}

/// The slug a `prereqs` item `[[<slug>]]` names, normalised as a link's (§5.2).
pub fn prereq_slug(item: &str) -> Option<Slug> {
    crate::path::nfc(item.strip_prefix("[[")?.strip_suffix("]]")?).parse().ok()
}

/// A concept frontmatter field (§6.1): the current ones, and those a vault update
/// removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConceptField {
    Id,
    Prereqs,
    Digest,
}

impl ConceptField {
    /// The field's key.
    pub const fn key(self) -> &'static str {
        match self {
            ConceptField::Id => "id",
            ConceptField::Prereqs => "prereqs",
            ConceptField::Digest => "digest",
        }
    }
}

/// A change to a concept's frontmatter (§6.1), by key or slug rather than by bytes, so
/// changes to one block compose: [`Note::rewrite_frontmatter`] applies a note's changes in
/// one rewrite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FmChange {
    /// Sets `id`, as the first key when there is none.
    SetId(Ulid),
    RemoveField(ConceptField),
    /// Quotes every unquoted `[[slug]]` in `prereqs` (YAML reads one as a list holding
    /// a list holding the slug).
    QuotePrereqs,
    /// Renames prereq `from` to `to`, or removes it; a `prereqs` list left empty loses
    /// its key.
    RenamePrereq {
        from: Slug,
        to: Option<Slug>,
    },
}

impl Note {
    /// The edit that rewrites the note's frontmatter (§6.1) with `changes`: the whole block
    /// in saphyr's canonical form, comments not kept. A note without frontmatter gets a
    /// block at the start of its body; a mapping left empty loses its block.
    ///
    /// # Panics
    /// When the frontmatter is broken (unterminated or not a mapping): it is not edited.
    pub fn rewrite_frontmatter(&self, changes: &[FmChange]) -> Edit {
        let (span, yaml) = match &self.frontmatter {
            None => (self.body_start()..self.body_start(), ""),
            Some(Spanned { node: Frontmatter::Fields { yaml, .. }, span }) => (span.clone(), &self.text[yaml.clone()]),
            Some(_) => panic!("{}: broken frontmatter is not rewritten", self.path),
        };

        let docs = Yaml::load_from_str(yaml).expect("a parsed frontmatter loads");
        let mut map = match docs.into_iter().next() {
            Some(Yaml::Mapping(map)) => map,
            None => Mapping::new(),
            Some(other) => panic!("{}: frontmatter is not a mapping: {other:?}", self.path),
        };

        // Renames go in one sweep, so `a -> b` beside `b -> c` renames each item once.
        let mut renames: BTreeMap<&Slug, Option<&Slug>> = BTreeMap::new();
        for change in changes {
            match change {
                FmChange::SetId(id) => set_id(&mut map, *id),
                FmChange::RemoveField(field) => _ = map.remove(&string_node(field.key())),
                FmChange::QuotePrereqs => quote_prereqs(&mut map),
                FmChange::RenamePrereq { from, to } => _ = renames.insert(from, to.as_ref()),
            }
        }
        if !renames.is_empty() {
            rename_prereqs(&mut map, &renames);
        }
        if map.is_empty() {
            // Nothing to write: the block goes, or (with none) nothing changes.
            let range = if self.frontmatter.is_some() { self.frontmatter_reach() } else { span };
            return Edit::replace(range, "");
        }

        Edit::replace(span, emit(map))
    }

    /// The bytes a rewrite of the frontmatter may replace: the block, or where one goes,
    /// and the blank lines after it, which go with a removed block so the note starts
    /// with its text.
    pub fn frontmatter_reach(&self) -> Range<usize> {
        let block = self.frontmatter.as_ref().map_or(self.body_start()..self.body_start(), |f| f.span.clone());
        block.start..block.end + blank_lines(&self.text[block.end..])
    }
}

/// The frontmatter of a new concept (m0 §9.7): its id and empty prereqs, written as
/// every rewrite writes frontmatter.
pub fn new_concept_frontmatter(id: Ulid) -> String {
    let mut map = Mapping::new();
    map.insert(string_node(ConceptField::Id.key()), string_node(id.to_string()));
    map.insert(string_node(ConceptField::Prereqs.key()), Yaml::Sequence(vec![]));
    emit(map)
}

/// The frontmatter block holding `map`, in saphyr's canonical form (§6.1): the one
/// writer of frontmatter.
fn emit(map: Mapping) -> String {
    // `---` and the mapping, without a final line break.
    let mut out = String::new();
    YamlEmitter::new(&mut out).dump(&Yaml::Mapping(map)).expect("writing to a string cannot fail");
    out + "\n---\n"
}

/// The length of the whitespace-only lines, breaks included, at the start of `text`.
fn blank_lines(text: &str) -> usize {
    text.split_inclusive('\n').take_while(|l| l.trim().is_empty() && l.ends_with('\n')).map(str::len).sum()
}

fn set_id(map: &mut Mapping, id: Ulid) {
    let (key, value) = (string_node(ConceptField::Id.key()), string_node(id.to_string()));
    match map.get_mut(&key) {
        Some(old) => *old = value,
        None => {
            let rest = std::mem::take(map);
            map.insert(key, value);
            map.extend(rest);
        }
    }
}

/// Quotes every unquoted `[[slug]]` in `prereqs` (the parsed form is
/// [`FmValue::is_unquoted_prereq`]'s).
fn quote_prereqs(map: &mut Mapping) {
    let Some(Yaml::Sequence(items)) = map.get_mut(&string_node(ConceptField::Prereqs.key())) else { return };
    for item in items {
        let Yaml::Sequence(outer) = item else { continue };
        let [Yaml::Sequence(inner)] = outer.as_slice() else { continue };
        let [Yaml::Value(Scalar::String(slug))] = inner.as_slice() else { continue };
        if let Ok(slug) = slug.parse::<Slug>() {
            *item = prereq_item(&slug);
        }
    }
}

/// Renames or removes the `prereqs` items named in `renames` (slug -> new slug, or
/// `None`); a list left empty loses its key.
fn rename_prereqs(map: &mut Mapping, renames: &BTreeMap<&Slug, Option<&Slug>>) {
    let key = string_node(ConceptField::Prereqs.key());
    let Some(Yaml::Sequence(items)) = map.get_mut(&key) else { return };
    items.retain_mut(|item| {
        let Yaml::Value(Scalar::String(s)) = item else { return true };
        match prereq_slug(s).and_then(|slug| renames.get(&slug).copied()) {
            None => true,
            Some(None) => false,
            Some(Some(new)) => {
                *item = prereq_item(new);
                true
            }
        }
    });
    if items.is_empty() {
        map.remove(&key);
    }
}

/// The `prereqs` item naming `slug`.
fn prereq_item<'a>(slug: &Slug) -> Yaml<'a> {
    string_node(format!("[[{slug}]]"))
}

fn string_node<'a>(s: impl Into<String>) -> Yaml<'a> {
    Yaml::Value(Scalar::String(s.into().into()))
}

/// Parses frontmatter YAML into top-level entries with byte spans relative to the file
/// (`offset` is the byte offset of `yaml` in the file).
fn parse_yaml(yaml: &str, offset: usize) -> Result<Vec<FmEntry>, String> {
    // `info()` is the message without the position (§7.1: messages carry no positions).
    let docs = MarkedYaml::load_from_str(yaml).map_err(|e| e.info().to_string())?;
    let Some(doc) = docs.into_iter().next() else { return Ok(vec![]) };
    let YamlData::Mapping(map) = &doc.data else {
        return Err("the top level is not a mapping".into());
    };
    let yaml = Source::new(yaml, offset);
    let mut entries = vec![];
    for (k, v) in map.iter() {
        let Some(key) = string(k) else { return Err("a key is not a string".into()) };
        entries.push(FmEntry { key: Spanned::new(key.to_string(), yaml.span(k)), value: yaml.value(v) });
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
struct Source {
    offset: usize,
    /// Character index -> byte index in `text`.
    bytes: Vec<usize>,
}

impl Source {
    fn new(text: &str, offset: usize) -> Self {
        let mut bytes: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
        bytes.push(text.len());
        Source { offset, bytes }
    }

    fn span(&self, n: &MarkedYaml) -> Range<usize> {
        let byte = |ci: usize| self.offset + self.bytes[ci];
        byte(n.span.start.index())..byte(n.span.end.index())
    }

    fn value(&self, n: &MarkedYaml) -> Spanned<FmValue> {
        let value = match (string(n), &n.data) {
            (Some(s), _) => FmValue::Str(s.to_string()),
            (None, YamlData::Sequence(items)) => FmValue::List(items.iter().map(|i| self.value(i)).collect()),
            (None, _) => FmValue::Other,
        };
        Spanned::new(value, self.span(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::testing::concept as p;

    #[test]
    fn frontmatter_spans() {
        let t = "---\nid: 01ABC\nprereqs: [\"[[a]]\", \"[[二分]]\"]\ndigest: \"é x\"\n---\n# T\n";
        let n = p(t);
        let fm = n.frontmatter.as_ref().unwrap();
        let digest = &fm.get("digest").unwrap().value;
        assert_eq!((&digest.node, digest.text(t)), (&FmValue::Str("é x".into()), "\"é x\""));
        let prereqs: Vec<_> = n.links.iter().filter(|l| l.form == LinkForm::Prereq).collect();
        assert_eq!(prereqs.len(), 2);
        assert_eq!(&t[prereqs[1].target.span.clone()], "\"[[二分]]\"");
        assert_eq!(prereqs[1].target.names, Names::Slug("二分".parse().unwrap()));
    }

    #[test]
    fn any_string_spelling_is_a_prereq() {
        let n = p("---\nprereqs:\n  - '[[a]]'\n  - \"[[\\u0062]]\"\n---\n# T\n");
        let names: Vec<_> = n.links.iter().map(|l| l.target.names.name().unwrap()).collect();
        assert_eq!(names, ["a", "b"]);
    }

    fn rewritten(text: &str, changes: &[FmChange]) -> String {
        let n = p(text);
        crate::splice::apply(text, vec![n.rewrite_frontmatter(changes)]).unwrap()
    }

    const ID: &str = "01JB9XQ7Z3K4M5N6P7Q8R9S0TV";

    fn set_id() -> FmChange {
        FmChange::SetId(ID.parse().unwrap())
    }

    #[test]
    fn rewrite_is_canonical_and_composes() {
        let t = "---\ndigest: x # why\nprereqs: [\"[[a]]\", [[b]], '[[c]]']\n---\n# T\n";
        let rename = |from: &str, to: Option<&str>| FmChange::RenamePrereq {
            from: from.parse().unwrap(),
            to: to.map(|t| t.parse().unwrap()),
        };
        let changes = [
            set_id(),
            FmChange::QuotePrereqs,
            rename("a", Some("c")),
            rename("c", None),
            FmChange::RemoveField(ConceptField::Digest),
        ];
        let out = rewritten(t, &changes);
        assert_eq!(out, format!("---\nid: {ID}\nprereqs:\n  - \"[[c]]\"\n  - \"[[b]]\"\n---\n# T\n"));
        assert_eq!(p(&out).links.len(), 2, "the rewrite parses back: {out}");
    }

    #[test]
    fn rewrite_adds_and_drops_the_block() {
        let added = rewritten("# T\n", &[set_id()]);
        assert_eq!(added, format!("---\nid: {ID}\n---\n# T\n"));
        let remove = [FmChange::RemoveField(ConceptField::Id)];
        assert_eq!(rewritten(&added, &remove), "# T\n");
        let spaced = format!("---\nid: {ID}\n---\n\n \r\n\t\n# T\n");
        assert_eq!(rewritten(&spaced, &remove), "# T\n", "the blank lines after a removed block go with it");
        let code = format!("---\nid: {ID}\n---\n\n    code\n");
        assert_eq!(rewritten(&code, &remove), "    code\n", "not the indentation of the first line");
        assert_eq!(rewritten("\n\n# T\n", &remove), "\n\n# T\n", "no block: nothing changes");
        assert_eq!(rewritten("\u{feff}# T\n", &[set_id()]), format!("\u{feff}---\nid: {ID}\n---\n# T\n"));
        let one = format!("---\nid: {ID}\nprereqs:\n  - \"[[a]]\"\n---\n# T\n");
        let gone = [FmChange::RenamePrereq { from: "a".parse().unwrap(), to: None }];
        assert_eq!(rewritten(&one, &gone), format!("---\nid: {ID}\n---\n# T\n"), "an emptied list loses its key");
    }

    #[test]
    fn unquoted_prereq_is_nested_list() {
        let n = p("---\nprereqs: [[a]]\n---\n# T\n");
        let fm = n.markdown.frontmatter.unwrap();
        let FmValue::List(items) = &fm.get("prereqs").unwrap().value.node else { panic!() };
        assert!(matches!(items[0].node, FmValue::List(_)));
    }

    #[test]
    fn duplicate_keys_rejected() {
        let n = p("---\nid: a\nid: b\n---\n# T\n");
        assert!(matches!(n.markdown.frontmatter.unwrap().node, Frontmatter::Invalid(_)));
    }

    #[test]
    fn unterminated() {
        let n = p("---\nid: a\n# T\n");
        assert_eq!(n.frontmatter, Some(Spanned::new(Frontmatter::Unterminated, 0..4)));
    }
}
