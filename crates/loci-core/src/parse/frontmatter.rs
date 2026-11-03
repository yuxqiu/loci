//! The frontmatter (m0 §4.3, §6.1): the YAML metadata block, read with the byte span of
//! every key and value, the `prereqs` links in it, and rewritten whole when loci edits it.

use std::collections::BTreeMap;
use std::ops::Range;

use saphyr::{LoadableYamlNode, Mapping, MarkedYaml, Scalar, Yaml, YamlData, YamlEmitter};

use super::body::Walker;
use super::links::prereq;
use super::*;
use crate::splice::Edit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FmValue {
    Str {
        value: String,
        span: Range<usize>,
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

/// A note's frontmatter (§4.3): what the YAML block at its start is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frontmatter {
    /// Opened with a `---` line that is never closed.
    Unterminated,
    /// A closed block (`span`, `---` lines and their breaks included) that is not a YAML
    /// mapping.
    Invalid { span: Range<usize>, reason: String },
    /// A YAML mapping: the block (as for `Invalid`) and its fields.
    Fields { span: Range<usize>, entries: Vec<FmEntry> },
}

impl Frontmatter {
    /// The bytes of the block, when it is closed.
    pub fn span(&self) -> Option<Range<usize>> {
        match self {
            Frontmatter::Unterminated => None,
            Frontmatter::Invalid { span, .. } | Frontmatter::Fields { span, .. } => Some(span.clone()),
        }
    }

    /// The fields: none unless the block is a YAML mapping.
    pub fn entries(&self) -> &[FmEntry] {
        match self {
            Frontmatter::Fields { entries, .. } => entries,
            Frontmatter::Unterminated | Frontmatter::Invalid { .. } => &[],
        }
    }

    pub fn get(&self, key: &str) -> Option<&FmEntry> {
        self.entries().iter().find(|e| e.key == key)
    }
}

/// The frontmatter (§4.3): a YAML metadata block at the start of the file.
pub(super) fn frontmatter(text: &str, base: usize, body: &Walker) -> Option<Frontmatter> {
    let Some(span) = body.meta_block.clone().filter(|r| r.start == base) else {
        let rest = &text[base..];
        let unterminated = rest.starts_with("---\n") || rest.starts_with("---\r\n") || rest == "---";
        return unterminated.then_some(Frontmatter::Unterminated);
    };
    // The closing `---` line's break too, so the body starts after it.
    let rest = &text[span.end..];
    let line_break = ["\r\n", "\n"].into_iter().find(|b| rest.starts_with(b)).map_or(0, str::len);
    let span = span.start..span.end + line_break;
    let yaml = body.meta_text.clone().unwrap_or(span.start..span.start);
    Some(match parse_yaml(&text[yaml.clone()], yaml.start) {
        Ok(entries) => Frontmatter::Fields { span, entries },
        Err(reason) => Frontmatter::Invalid { span, reason },
    })
}

/// The `prereqs` links (§6.1): each string item `[[<slug>]]`. Any other item is no link,
/// and lint reports it (`fm.field-type`).
pub(super) fn frontmatter_links(fm: &Frontmatter) -> Vec<Link> {
    let items = match fm.get("prereqs") {
        Some(FmEntry { value: FmValue::List { items, .. }, .. }) => items.as_slice(),
        _ => &[],
    };
    let slug = |item: &FmValue| match item {
        FmValue::Str { value, span } => Some(prereq(span.clone(), prereq_slug(value)?)),
        _ => None,
    };
    items.iter().filter_map(slug).collect()
}

/// The slug a `prereqs` item `[[<slug>]]` names, normalised as a link's (§5.2).
pub fn prereq_slug(item: &str) -> Option<String> {
    let slug = item.strip_prefix("[[")?.strip_suffix("]]")?.parse::<crate::id::Slug>().ok()?;
    Some(crate::vault::nfc(&slug.to_string()))
}

/// The `prereqs` item naming `slug`.
pub fn prereq_item<'a>(slug: &str) -> Yaml<'a> {
    yaml_string(format!("[[{slug}]]"))
}

/// Renames or removes the `prereqs` items of `map` named in `renames` (slug -> new
/// slug, or `None` to remove it); a list left empty loses its key.
pub fn rename_prereqs(map: &mut Mapping, renames: &BTreeMap<&str, Option<&str>>) {
    let key = yaml_string("prereqs");
    let Some(Yaml::Sequence(items)) = map.get_mut(&key) else { panic!("a note with prereqs has a `prereqs` list") };
    items.retain_mut(|item| {
        let Yaml::Value(Scalar::String(s)) = item else { return true };
        match prereq_slug(s).and_then(|slug| renames.get(slug.as_str()).copied()) {
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

/// A YAML string, for a key or value of a [`rewrite_frontmatter`].
pub fn yaml_string<'a>(s: impl Into<String>) -> Yaml<'a> {
    Yaml::Value(Scalar::String(s.into().into()))
}

/// The edit that rewrites `note`'s frontmatter (§6.1) after `change` edits its YAML
/// mapping: the whole block in saphyr's canonical form, comments not kept. A note
/// without frontmatter gets a block at the start of its body; a mapping left empty
/// loses its block.
///
/// # Panics
/// When the frontmatter is broken (unterminated or not a mapping): it is not edited.
pub fn rewrite_frontmatter<'t>(note: &'t Note, change: impl FnOnce(&mut Mapping<'t>)) -> Edit {
    let (span, block) = match &note.frontmatter {
        None => (note.body_start()..note.body_start(), ""),
        Some(Frontmatter::Fields { span, .. }) => (span.clone(), &note.text[span.clone()]),
        Some(_) => panic!("{}: broken frontmatter is not rewritten", note.path),
    };

    // The block is a YAML document between `---` lines; the closing line opens an
    // empty second one.
    let docs = Yaml::load_from_str(block).expect("a parsed frontmatter loads");
    let mut map = match docs.into_iter().next() {
        Some(Yaml::Mapping(map)) => map,
        None | Some(Yaml::Value(Scalar::Null)) => Mapping::new(),
        Some(other) => panic!("{}: frontmatter is not a mapping: {other:?}", note.path),
    };

    change(&mut map);
    if map.is_empty() {
        return Edit::replace(span, "");
    }

    // `---` and the mapping, without a final line break.
    let mut out = String::new();
    YamlEmitter::new(&mut out).dump(&Yaml::Mapping(map)).expect("writing to a string cannot fail");
    Edit::replace(span, out + "\n---\n")
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
    let yaml = Source::new(yaml, offset);
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
struct Source<'a> {
    text: &'a str,
    offset: usize,
    /// Character index -> byte index in `text`.
    bytes: Vec<usize>,
}

impl<'a> Source<'a> {
    fn new(text: &'a str, offset: usize) -> Self {
        let mut bytes: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
        bytes.push(text.len());
        Source { text, offset, bytes }
    }

    fn span(&self, n: &MarkedYaml) -> Range<usize> {
        let byte = |ci: usize| self.offset + self.bytes[ci];
        byte(n.span.start.index())..byte(n.span.end.index())
    }

    fn value(&self, n: &MarkedYaml) -> FmValue {
        let span = self.span(n);
        let source = &self.text[span.start - self.offset..];
        if let Some(s) = string(n) {
            return FmValue::Str { value: s.to_string(), span };
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
    use crate::parse::{Note, parse};
    use crate::path::VaultPath;
    use crate::vault::Kind;

    fn p(text: &str) -> Note {
        parse(&VaultPath::new("concepts/a/x.md".into()).unwrap(), Kind::Concept, text.to_string())
    }

    #[test]
    fn frontmatter_spans() {
        let t = "---\nid: 01ABC\nprereqs: [\"[[a]]\", \"[[二分]]\"]\ndigest: \"é x\"\n---\n# T\n";
        let n = p(t);
        let fm = n.frontmatter.as_ref().unwrap();
        let FmValue::Str { span, .. } = &fm.get("digest").unwrap().value else { panic!() };
        assert_eq!(&t[span.clone()], "\"é x\"");
        let prereqs: Vec<_> = n.links.iter().filter(|l| l.form == LinkForm::Prereq).collect();
        assert_eq!(prereqs.len(), 2);
        assert_eq!(&t[prereqs[1].target.span.clone()], "\"[[二分]]\"");
        assert_eq!(prereqs[1].target.names, Names::Slug("二分".into()));
    }

    #[test]
    fn any_string_spelling_is_a_prereq() {
        let n = p("---\nprereqs:\n  - '[[a]]'\n  - \"[[\\u0062]]\"\n---\n# T\n");
        let names: Vec<_> = n.links.iter().map(|l| l.target.names.name().unwrap()).collect();
        assert_eq!(names, ["a", "b"]);
    }

    fn rewritten(text: &str, change: impl FnOnce(&mut Mapping)) -> String {
        let n = p(text);
        crate::splice::apply(text, vec![rewrite_frontmatter(&n, change)]).unwrap()
    }

    #[test]
    fn rewrite_is_canonical() {
        let t = "---\nid: X # the id\nprereqs: [\"[[a]]\", '[[b]]']\n---\n# T\n";
        let out = rewritten(t, |m| {
            m.insert(yaml_string("tags"), yaml_string("t"));
        });
        assert_eq!(out, "---\nid: X\nprereqs:\n  - \"[[a]]\"\n  - \"[[b]]\"\ntags: t\n---\n# T\n");
        let p2 = p(&out);
        assert_eq!(p2.links.len(), 2, "the rewrite parses back: {out}");
    }

    #[test]
    fn rewrite_adds_and_drops_the_block() {
        let added = rewritten("# T\n", |m| {
            m.insert(yaml_string("id"), yaml_string("X"));
        });
        assert_eq!(added, "---\nid: X\n---\n# T\n");
        assert_eq!(rewritten(&added, |m| m.clear()), "# T\n");
        assert_eq!(
            rewritten("\u{feff}# T\n", |m| _ = m.insert(yaml_string("id"), yaml_string("X"))),
            "\u{feff}---\nid: X\n---\n# T\n"
        );
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
        assert!(matches!(n.frontmatter, Some(Frontmatter::Invalid { .. })));
    }

    #[test]
    fn unterminated() {
        let n = p("---\nid: a\n# T\n");
        assert_eq!(n.frontmatter, Some(Frontmatter::Unterminated));
    }
}
