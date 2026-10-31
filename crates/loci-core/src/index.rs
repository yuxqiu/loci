//! The vault index: every file core reads, parsed once, plus the name tables used for
//! link resolution (m0 §5).

use std::collections::{BTreeMap, BTreeSet};

use unicode_normalization::UnicodeNormalization;

use crate::parse::{self, Link, LinkForm, Note};
use crate::vault::{self, Kind};

/// A file that could not be read as a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileIssue {
    NotUtf8,
    /// Reading or listing failed, or the name is not UTF-8; the reason.
    Unreadable(String),
}

/// Why [`Index::concept`] found no single concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConceptLookup {
    Unknown,
    /// The slug names several concepts (lint reports `slug.duplicate`).
    Ambiguous(Vec<String>),
}

impl ConceptLookup {
    /// The refusal code and message for argument `arg`: `unknown_code` when no concept
    /// matches, `concept.ambiguous` when several do.
    pub fn refusal(&self, unknown_code: &'static str, arg: &str) -> (&'static str, String) {
        match self {
            ConceptLookup::Unknown => (unknown_code, format!("no concept `{arg}`")),
            ConceptLookup::Ambiguous(paths) => (
                "concept.ambiguous",
                format!("`{arg}` names several concepts ({}); name one by path", paths.join(", ")),
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Index {
    /// Every file core reads, sorted.
    pub files: Vec<String>,
    /// Parsed `.md` files, by path.
    pub notes: BTreeMap<String, Note>,
    pub issues: BTreeMap<String, FileIssue>,
    /// Lowercased concept slug -> concept paths.
    pub slugs: BTreeMap<String, Vec<String>>,
    /// Lowercased alias -> concept paths declaring it.
    pub aliases: BTreeMap<String, Vec<String>>,
    /// Link targets by path: notes without `.md`, other files with their extension.
    pub targets: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// A note, by path.
    Note(String),
    /// A non-markdown file, by path.
    File(String),
}

impl Resolved {
    pub fn path(&self) -> &str {
        match self {
            Resolved::Note(p) | Resolved::File(p) => p,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    Dangling { hint: Option<String> },
    PathForm,
    Heading,
    Block,
}

/// An index of in-memory files: `.md` texts are parsed, other contents are unused.
pub fn from_texts(files: &[(&str, &str)]) -> Index {
    let map: BTreeMap<&str, &str> = files.iter().copied().collect();
    build(map.keys().map(|p| p.to_string()).collect(), |p| {
        map.get(p).map(|t| t.as_bytes().to_vec()).ok_or_else(|| std::io::Error::other("not in memory"))
    })
}

/// Indexes `files` (sorted vault paths), reading each `.md` file with `read`.
pub fn build(files: Vec<String>, read: impl Fn(&str) -> std::io::Result<Vec<u8>>) -> Index {
    let mut index = Index {
        files: files.clone(),
        notes: BTreeMap::new(),
        issues: BTreeMap::new(),
        slugs: BTreeMap::new(),
        aliases: BTreeMap::new(),
        targets: BTreeSet::new(),
    };
    for path in files {
        let kind = vault::kind_of(&path).unwrap_or(Kind::Unknown);
        if let Some(stripped) = path.strip_suffix(".md") {
            index.targets.insert(stripped.to_string());
            let bytes = match read(&path) {
                Ok(b) => b,
                Err(e) => {
                    index.issues.insert(path, FileIssue::Unreadable(e.to_string()));
                    continue;
                }
            };
            match String::from_utf8(bytes) {
                Ok(text) => {
                    let note = parse::parse(&path, kind, text);
                    if kind == Kind::Concept {
                        let slug = vault::stem(&path).to_lowercase();
                        index.slugs.entry(slug).or_default().push(path.clone());
                        for alias in note.fm_strings("aliases") {
                            let a: String = alias.nfc().collect::<String>().to_lowercase();
                            index.aliases.entry(a).or_default().push(path.clone());
                        }
                    }
                    index.notes.insert(path, note);
                }
                Err(_) => {
                    index.issues.insert(path, FileIssue::NotUtf8);
                }
            }
        } else {
            index.targets.insert(path);
        }
    }
    index
}

impl Index {
    pub fn open(vault: &vault::Vault) -> Self {
        let root = vault.root.clone();
        let walk = vault.walk();
        let mut index = build(walk.files, |p| std::fs::read(root.join(p)));
        for (path, reason) in walk.unreadable {
            index.issues.insert(path, FileIssue::Unreadable(reason));
        }
        index
    }

    /// The concept a user argument names: a concept path (`.md` optional) or a slug
    /// (case-insensitive). The argument is put in NFC. A slug shared by several
    /// concepts is ambiguous.
    pub fn concept(&self, arg: &str) -> Result<&Note, ConceptLookup> {
        let arg = vault::nfc(arg);
        let path = if arg.ends_with(".md") { arg.clone() } else { format!("{arg}.md") };
        if let Some(n) = self.notes.get(&path).filter(|n| n.kind == Kind::Concept) {
            return Ok(n);
        }
        match self.slugs.get(&arg.to_lowercase()).map(Vec::as_slice) {
            None | Some([]) => Err(ConceptLookup::Unknown),
            Some([p]) => self.notes.get(p).ok_or(ConceptLookup::Unknown),
            Some(ps) => Err(ConceptLookup::Ambiguous(ps.to_vec())),
        }
    }

    /// The `prereqs` of concept `note`: resolved note paths, and the raw targets that do
    /// not resolve.
    pub fn prereqs(&self, note: &Note) -> (Vec<String>, Vec<String>) {
        let (mut resolved, mut unresolved) = (vec![], vec![]);
        for l in note.links.iter().filter(|l| l.form == LinkForm::Prereq) {
            match self.resolve(note, l) {
                Ok(Resolved::Note(p)) => resolved.push(p),
                _ => unresolved.push(l.raw.clone()),
            }
        }
        (resolved, unresolved)
    }

    /// Area names (m0 §6.3): the folders `concepts/<area>/` that hold a concept or an
    /// `AREA.md`.
    pub fn areas(&self) -> BTreeSet<&str> {
        self.notes
            .values()
            .filter(|n| matches!(n.kind, Kind::Concept | Kind::Area))
            .filter_map(|n| vault::area_of(&n.path))
            .collect()
    }

    /// Where `link` in note `from` points, ignoring its fragment: a note or file path,
    /// a source folder (`sources/<type>/<slug>`), or `None` when it does not resolve.
    pub fn link_target(&self, from: &Note, link: &Link) -> Option<String> {
        if link.form != LinkForm::Source {
            let whole = Link { fragment: None, ..link.clone() };
            return self.resolve(from, &whole).ok().map(|r| r.path().to_string());
        }
        let note = format!("{}.md", link.target);
        let folder = format!("{}/", link.target);
        if self.notes.contains_key(&note) {
            Some(note)
        } else if self.files.iter().any(|f| f.starts_with(&folder)) {
            Some(link.target.clone())
        } else {
            None
        }
    }

    /// Every link that points at `path`, or at the source folder holding it, as
    /// (linking note, link), in note path order.
    pub fn incoming(&self, path: &str) -> Vec<(&Note, &Link)> {
        let mut out = vec![];
        for note in self.notes.values() {
            for link in &note.links {
                let Some(target) = self.link_target(note, link) else { continue };
                if target == path || path.strip_prefix(&target).is_some_and(|rest| rest.starts_with('/')) {
                    out.push((note, link));
                }
            }
        }
        out
    }

    pub fn concepts(&self) -> impl Iterator<Item = &Note> {
        self.notes.values().filter(|n| n.kind == Kind::Concept)
    }

    /// Resolves a body or `prereqs` link from note `from` (§5.3, §5.4). `Source` links
    /// are resolved by [`Index::resolve_source`].
    pub fn resolve(&self, from: &Note, link: &Link) -> Result<Resolved, LinkError> {
        let target = &link.target;
        let resolved = if target.is_empty() {
            // `[[#frag]]`: the linking note itself.
            Resolved::Note(from.path.clone())
        } else if !target.contains('/') {
            let key = target.to_lowercase();
            match self.slugs.get(&key) {
                Some(paths) => Resolved::Note(paths[0].clone()),
                None => {
                    let hint = match self.aliases.get(&key) {
                        Some(paths) => Some(format!("alias of [[{}]]", vault::stem(&paths[0]))),
                        None => Some("link non-concepts by path".to_string()),
                    };
                    return Err(LinkError::Dangling { hint });
                }
            }
        } else {
            if target.starts_with('/') || target.split('/').any(|s| s == "..") {
                return Err(LinkError::PathForm);
            }
            if !self.targets.contains(target) {
                return Err(LinkError::Dangling { hint: None });
            }
            let note = format!("{target}.md");
            if self.files.binary_search(&note).is_ok() { Resolved::Note(note) } else { Resolved::File(target.clone()) }
        };
        if let (Some(frag), Resolved::Note(path)) = (&link.fragment, &resolved) {
            self.check_fragment(path, frag)?;
        }
        Ok(resolved)
    }

    /// Checks a `#fragment` against the headings or block ids of note `path` (§5.4).
    pub fn check_fragment(&self, path: &str, frag: &str) -> Result<(), LinkError> {
        // An unreadable note is reported on its own path (`file.*`); its fragments
        // cannot be checked.
        let Some(note) = self.notes.get(path) else { return Ok(()) };
        if let Some(id) = frag.strip_prefix('^') {
            if note.block_ids.iter().any(|b| b.id == id) {
                return Ok(());
            }
            return Err(LinkError::Block);
        }
        let key = heading_key(frag);
        if note.headings.iter().any(|h| heading_key(&h.text) == key) { Ok(()) } else { Err(LinkError::Heading) }
    }
}

/// The comparison key for heading fragments (§5.4).
pub fn heading_key(s: &str) -> String {
    s.to_lowercase().replace(' ', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(files: &[(&str, &str)]) -> Index {
        crate::index::from_texts(files)
    }

    fn first_link(i: &Index, path: &str) -> (Note, Link) {
        let n = i.notes[path].clone();
        let l = n.links[0].clone();
        (n, l)
    }

    #[test]
    fn bare_names_resolve_to_concepts_only() {
        let i = idx(&[
            ("concepts/a/bfs.md", "---\naliases: [breadth]\n---\n# BFS\n"),
            ("views/learn/bfs.md", "# Plan\n\n[[bfs]] [[plan]] [[breadth]]\n"),
            ("views/plan.md", "# P\n"),
        ]);
        let n = i.notes["views/learn/bfs.md"].clone();
        assert_eq!(i.resolve(&n, &n.links[0]), Ok(Resolved::Note("concepts/a/bfs.md".into())));
        assert!(matches!(i.resolve(&n, &n.links[1]), Err(LinkError::Dangling { .. })));
        assert_eq!(i.resolve(&n, &n.links[2]), Err(LinkError::Dangling { hint: Some("alias of [[bfs]]".into()) }));
    }

    #[test]
    fn paths_and_fragments() {
        let i = idx(&[
            ("views/v.md", "# V\n\n## Steps\n\n- one ^abc\n"),
            (
                "journal/r/2025/03/25-03-14.md",
                "# J\n\n[[views/v#steps]] [[views/v#^abc]] [[views/v#nope]] [[../x]] [[/views/v]]\n",
            ),
        ]);
        let n = i.notes["journal/r/2025/03/25-03-14.md"].clone();
        let r: Vec<_> = n.links.iter().map(|l| i.resolve(&n, l)).collect();
        assert!(r[0].is_ok());
        assert!(r[1].is_ok());
        assert_eq!(r[2], Err(LinkError::Heading));
        assert_eq!(r[3], Err(LinkError::PathForm));
        assert_eq!(r[4], Err(LinkError::PathForm));
    }

    #[test]
    fn embeds_of_files() {
        let i = idx(&[
            ("concepts/a/x.md", "---\nid: X\n---\n# X\n\n![[concepts/a/attachments/p.png|300]]\n"),
            ("concepts/a/attachments/p.png", ""),
        ]);
        let (n, l) = first_link(&i, "concepts/a/x.md");
        assert_eq!(i.resolve(&n, &l), Ok(Resolved::File("concepts/a/attachments/p.png".into())));
    }
}
