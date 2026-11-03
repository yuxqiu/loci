//! Links (m0 §5): wikilinks and embeds, their targets and fragments, and markdown links.

use std::ops::Range;

use super::Spanned;
use crate::id::Slug;

/// Serialized in lowercase (`wiki`, `embed`, `prereq`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkForm {
    Wiki,
    Embed,
    Prereq,
}

/// A link in the vault's sense (§5.1): a wikilink, an embed or a `prereqs` item, written
/// `[[target#fragment|label]]` (`![[…]]` for an embed; a prereq is a YAML string
/// `[[slug]]`). The index resolves it (§5.3), `mv` and `merge` rewrite its target,
/// export follows it. Made by `wikilink` or `prereq`, spanning the whole `[[…]]`;
/// a prereq spans its YAML string, quotes included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub form: LinkForm,
    /// Spanning the target as written, fragment included; renames replace it.
    pub target: Spanned<Target>,
    pub label: Option<Spanned<Label>>,
}

/// What a link points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub names: Names,
    pub fragment: Option<Fragment>,
}

/// A link's label, `[[target|label]]`, or a markdown link's text: its value is its
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label;

/// How a target names a note or file (§5.2, §5.3): normalised (`.md` dropped, NFC).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Names {
    /// `[[#heading]]`: the linking note itself.
    ThisNote,
    /// `[[bfs]]`: a concept, by slug.
    Slug(Slug),
    /// `[[two words]]`: a bare name that is no slug, so it names no concept.
    Bare(String),
    /// `[[concepts/a/bfs]]`: any vault file, by path.
    Path(String),
    /// `[[../x]]`, `[[/x]]`: a path that climbs out or starts at the root, so it names
    /// nothing (`link.path-form`, §5.2).
    Escaping(String),
}

impl Names {
    /// The name as normalised; `None` for the linking note itself.
    pub fn name(&self) -> Option<&str> {
        match self {
            Names::ThisNote => None,
            Names::Slug(slug) => Some(slug.as_str()),
            Names::Bare(name) | Names::Path(name) | Names::Escaping(name) => Some(name),
        }
    }
}

impl Link {
    /// The concept a `prereqs` item names; `None` for any other link.
    pub fn prereq_slug(&self) -> Option<&Slug> {
        match (self.form, &self.target.names) {
            (LinkForm::Prereq, Names::Slug(slug)) => Some(slug),
            _ => None,
        }
    }
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

/// The comparison key for heading fragments (§5.4).
pub fn heading_key(s: &str) -> String {
    s.to_lowercase().replace(' ', "-")
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

/// A markdown-syntax link to a vault file (`[text](path)`, `![alt](path)`): not a link
/// loci follows, since a vault target must be a wikilink (§5.1); recorded so lint can
/// report it (`link.markdown`) and fix it into one. Links to URLs are not recorded.
/// Spans the whole link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownLink {
    pub url: String,
    pub image: bool,
    /// The link text (an image's alt text); empty when there is none.
    pub label: Spanned<Label>,
}

/// The wikilink (`[[target|label]]`) or embed (`![[…]]`) written at `range` of `text`
/// (§5.1), as a link of `form`: its target (a table's escaping `\` before the `|` left
/// out), fragment and label. `None` when the bytes are not exactly one wikilink, or it
/// has no target (`[[]]`). This is the one place that knows a wikilink's brackets.
pub(super) fn wikilink(text: &str, range: Range<usize>, form: LinkForm) -> Option<Spanned<Link>> {
    let open = if form == LinkForm::Embed { "![[" } else { "[[" };
    let start = range.start + open.len();
    let inner = text[range.clone()].strip_prefix(open)?.strip_suffix("]]")?;
    let close = inner.len();
    let pipe = inner.find('|');
    let label = pipe.map(|p| Spanned::new(Label, start + p + 1..start + close));
    // In a table the `|` is escaped (`\|`); the `\` is table syntax, not target.
    let target = &inner[..pipe.unwrap_or(close)];
    let target = target.strip_suffix('\\').filter(|_| pipe.is_some()).unwrap_or(target);
    Some(Spanned::new(Link { form, target: read_target(target, start)?, label }, range))
}

/// The prereq naming `slug`, the YAML string at `span` (§6.1). Its bytes need not spell
/// the slug (YAML has escapes), so the whole string is the link and its target; loci
/// rewrites it with the frontmatter, never in place.
pub(super) fn prereq(span: Range<usize>, slug: Slug) -> Spanned<Link> {
    let target = Spanned::new(Target { names: Names::Slug(slug), fragment: None }, span.clone());
    Spanned::new(Link { form: LinkForm::Prereq, target, label: None }, span)
}

/// The target `written` at byte `start` (§5.2): what it names and its fragment. `None`
/// for an empty one (`[[]]`).
fn read_target(written: &str, start: usize) -> Option<Spanned<Target>> {
    let s = written.trim_matches(|c: char| c.is_ascii_whitespace());
    let (name, fragment) = match s.split_once('#') {
        Some((name, frag)) => (name, Some(Fragment::parse(frag))),
        None => (s, None),
    };
    let name = crate::path::nfc(name.strip_suffix(".md").unwrap_or(name));
    let names = match name {
        _ if name.is_empty() && fragment.is_none() => return None,
        _ if name.is_empty() => Names::ThisNote,
        _ if name.starts_with('/') || name.split('/').any(|s| s == "..") => Names::Escaping(name),
        _ if name.contains('/') => Names::Path(name),
        _ => name.parse().map_or(Names::Bare(name), Names::Slug),
    };
    Some(Spanned::new(Target { names, fragment }, start..start + written.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Note, parse};
    use crate::path::Kind;
    use crate::path::VaultPath;

    fn p(text: &str) -> Note {
        parse(&VaultPath::new("concepts/a/x.md".into()).unwrap(), Kind::Concept, text.to_string())
    }

    #[test]
    fn targets() {
        let slug = |n: &str| Names::Slug(n.parse().unwrap());
        let t = read_target(" a.md#H ", 0).unwrap();
        assert_eq!((t.node.names, t.node.fragment), (slug("a"), Some(Fragment::Heading("H".into()))));
        assert_eq!(read_target("a#^b1", 0).unwrap().fragment, Some(Fragment::Block("b1".into())));
        assert_eq!(read_target("#h", 0).unwrap().names, Names::ThisNote);
        assert_eq!(read_target("x/y.png", 0).unwrap().names, Names::Path("x/y.png".into()));
        assert_eq!(read_target("", 0), None);
    }

    #[test]
    fn wikilinks() {
        let t = "# T\n\nSee [[b#H|label]] and ![[x/y.png|300]] and [m](a/b.md) and [w](https://x).\n";
        let n = p(t);
        assert_eq!(n.links.len(), 2);
        assert_eq!(n.links[0].target.text(t), "b#H");
        assert_eq!(n.links[0].target.names, Names::Slug("b".parse().unwrap()));
        assert_eq!(n.links[0].target.fragment, Some(Fragment::Heading("H".into())));
        assert_eq!(n.links[1].form, LinkForm::Embed);
        assert_eq!(n.links[1].target.names, Names::Path("x/y.png".into()));
        let labels: Vec<Option<&str>> = n.links.iter().map(|l| l.label.as_ref().map(|l| l.text(t))).collect();
        assert_eq!(labels, [Some("label"), Some("300")]);
        assert_eq!(n.markdown_links.len(), 1);
    }

    #[test]
    fn a_table_escape_is_outside_the_target() {
        let t = "# T\n\n| a | [[b\\|label]] |\n|---|---|\n";
        let n = p(t);
        let l = &n.links[0];
        assert_eq!((l.target.text(t), &l.target.names), ("b", &Names::Slug("b".parse().unwrap())));
        assert_eq!(l.label.as_ref().map(|l| l.text(t)), Some("label"));
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
        assert_eq!(n.links[0].target.names, Names::Slug("x".parse().unwrap()));
    }
}
