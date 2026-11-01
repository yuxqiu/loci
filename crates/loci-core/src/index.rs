//! The vault index: every file core reads, parsed once, plus the name tables used for
//! link resolution (m0 §5).

use std::collections::{BTreeMap, BTreeSet};

use crate::envelope::Refusal;
use crate::parse::{self, Link, LinkForm, Note};
use crate::vault::{self, Kind};

/// A file that could not be read as a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileIssue {
    NotUtf8,
    /// Reading or listing failed, or the name is not UTF-8; the reason.
    Unreadable(String),
}

/// A note argument as a note path: `.md` added when missing.
fn note_path(arg: &str) -> String {
    if arg.ends_with(".md") { arg.to_string() } else { format!("{arg}.md") }
}

/// Why [`Index::concept`] found no single concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConceptLookup {
    Unknown,
    /// The slug names several concepts (lint reports `slug.duplicate`).
    Ambiguous(Vec<String>),
    /// The argument names a note that is not a concept: its path.
    NotConcept(String),
}

impl ConceptLookup {
    /// The refusal for argument `arg`: `unknown_code` when no note matches or the note
    /// is not a concept, `concept.ambiguous` when a slug names several.
    pub fn refusal(&self, unknown_code: &str, arg: &str) -> Refusal {
        match self {
            ConceptLookup::Unknown => Refusal::refused(unknown_code, format!("no note named `{arg}`")),
            ConceptLookup::NotConcept(path) => Refusal::refused(unknown_code, format!("`{path}` is not a concept")),
            ConceptLookup::Ambiguous(paths) => Refusal::refused(
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
    /// Every markdown file core reads, parsed, by path: concepts, area files, views,
    /// journal entries, rule files, markdown sources and inbox drafts.
    pub markdown: BTreeMap<String, Note>,
    pub issues: BTreeMap<String, FileIssue>,
    /// Lowercased concept slug -> concept paths.
    pub slugs: BTreeMap<String, Vec<String>>,
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
pub fn from_texts(files: &[(impl AsRef<str>, impl AsRef<str>)]) -> Index {
    let map: BTreeMap<&str, &str> = files.iter().map(|(p, t)| (p.as_ref(), t.as_ref())).collect();
    build(map.keys().map(|p| p.to_string()).collect(), |p| {
        map.get(p).map(|t| t.as_bytes().to_vec()).ok_or_else(|| std::io::Error::other("not in memory"))
    })
}

/// Indexes `files` (sorted vault paths), reading each `.md` file with `read`.
pub fn build(files: Vec<String>, read: impl Fn(&str) -> std::io::Result<Vec<u8>>) -> Index {
    let mut index = Index {
        files: files.clone(),
        markdown: BTreeMap::new(),
        issues: BTreeMap::new(),
        slugs: BTreeMap::new(),
        targets: BTreeSet::new(),
    };
    for path in files {
        let kind = vault::kind_of(&path).expect("the index holds only paths core reads");
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
                    }
                    index.markdown.insert(path, note);
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

    /// The note a note argument names (m0 §8): a vault path to any note (`.md`
    /// optional), else a concept slug (case-insensitive). A slug shared by several
    /// concepts is ambiguous. `arg` is a [`vault::path_arg`] result.
    pub fn note(&self, arg: &str) -> Result<&Note, ConceptLookup> {
        if let Some(n) = self.markdown.get(&note_path(arg)) {
            return Ok(n);
        }
        match self.slugs.get(&arg.to_lowercase()).map(Vec::as_slice) {
            None | Some([]) => Err(ConceptLookup::Unknown),
            Some([p]) => Ok(self.markdown.get(p).expect("slugs are taken from parsed notes")),
            Some(ps) => Err(ConceptLookup::Ambiguous(ps.to_vec())),
        }
    }

    /// The concept a note argument names: [`Index::note`], refused when that note is
    /// not a concept.
    pub fn concept(&self, arg: &str) -> Result<&Note, ConceptLookup> {
        let note = self.note(arg)?;
        if note.kind == Kind::Concept { Ok(note) } else { Err(ConceptLookup::NotConcept(note.path.clone())) }
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
        self.markdown
            .values()
            .filter(|n| matches!(n.kind, Kind::Concept | Kind::Area))
            .filter_map(|n| vault::area_of(&n.path))
            .collect()
    }

    /// Where `link` in note `from` points, ignoring its fragment: a note or file path,
    /// or `None` when it does not resolve.
    pub fn link_target(&self, from: &Note, link: &Link) -> Option<String> {
        self.resolve_target(from, link).ok().map(|r| r.path().to_string())
    }

    /// Whether the vault has a file at `path`, or a folder (files under `path/`).
    pub fn has_entry(&self, path: &str) -> bool {
        let below = format!("{path}/");
        self.files.iter().any(|f| f == path || f.starts_with(&below))
    }

    /// Every link that points at `path`, as (linking note, link), in note path order.
    pub fn incoming(&self, path: &str) -> Vec<(&Note, &Link)> {
        let mut out = vec![];
        for note in self.markdown.values() {
            for link in &note.links {
                let Some(target) = self.link_target(note, link) else { continue };
                if target == path {
                    out.push((note, link));
                }
            }
        }
        out
    }

    /// Whether `slug` is already a concept's slug, other than paths `moving` accepts,
    /// which are being renamed (§9.5, §9.7).
    pub fn name_taken(&self, slug: &str, moving: impl Fn(&str) -> bool) -> bool {
        self.slugs.get(&slug.to_lowercase()).is_some_and(|ps| ps.iter().any(|p| !moving(p)))
    }

    pub fn concepts(&self) -> impl Iterator<Item = &Note> {
        self.markdown.values().filter(|n| n.kind == Kind::Concept)
    }

    /// Resolves a body or `prereqs` link from note `from` (§5.3), then checks its
    /// fragment (§5.4).
    pub fn resolve(&self, from: &Note, link: &Link) -> Result<Resolved, LinkError> {
        let resolved = self.resolve_target(from, link)?;
        if let (Some(frag), Resolved::Note(path)) = (&link.fragment, &resolved) {
            self.check_fragment(path, frag)?;
        }
        Ok(resolved)
    }

    /// The note or file `link` points at (§5.3), ignoring its fragment.
    fn resolve_target(&self, from: &Note, link: &Link) -> Result<Resolved, LinkError> {
        let target = &link.target;
        let resolved = if target.is_empty() {
            // `[[#frag]]`: the linking note itself.
            Resolved::Note(from.path.clone())
        } else if !target.contains('/') {
            let key = target.to_lowercase();
            match self.slugs.get(&key).map(Vec::as_slice) {
                Some([path]) => Resolved::Note(path.clone()),
                // A slug shared by several concepts (`slug.duplicate`) names none of them.
                Some(paths @ [_, _, ..]) => {
                    let hint = format!("several concepts are named `{key}` ({}); link by path", paths.join(", "));
                    return Err(LinkError::Dangling { hint: Some(hint) });
                }
                Some([]) | None => {
                    return Err(LinkError::Dangling { hint: Some("link non-concepts by path".to_string()) });
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
        Ok(resolved)
    }

    /// Checks a `#fragment` against the headings or block ids of note `path` (§5.4).
    pub fn check_fragment(&self, path: &str, frag: &str) -> Result<(), LinkError> {
        // An unreadable note is reported on its own path (`file.*`); its fragments
        // cannot be checked.
        let Some(note) = self.markdown.get(path) else { return Ok(()) };
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

    fn first_link(i: &Index, path: &str) -> (Note, Link) {
        let n = i.markdown[path].clone();
        let l = n.links[0].clone();
        (n, l)
    }

    #[test]
    fn bare_names_resolve_to_concepts_only() {
        let i = from_texts(&[
            ("concepts/a/bfs.md", "# BFS\n"),
            ("views/learn/bfs.md", "# Plan\n\n[[bfs]] [[plan]]\n"),
            ("views/plan.md", "# P\n"),
        ]);
        let n = i.markdown["views/learn/bfs.md"].clone();
        assert_eq!(i.resolve(&n, &n.links[0]), Ok(Resolved::Note("concepts/a/bfs.md".into())));
        assert!(matches!(i.resolve(&n, &n.links[1]), Err(LinkError::Dangling { .. })));
    }

    #[test]
    fn a_shared_slug_resolves_to_no_concept() {
        let i = from_texts(&[
            ("concepts/a/bfs.md", "# A\n"),
            ("concepts/b/bfs.md", "# B\n"),
            ("views/v.md", "# V\n\n[[bfs]] [[concepts/b/bfs]]\n"),
        ]);
        let n = i.markdown["views/v.md"].clone();
        let Err(LinkError::Dangling { hint: Some(hint) }) = i.resolve(&n, &n.links[0]) else { panic!("resolved") };
        assert!(hint.contains("link by path"), "{hint}");
        assert_eq!(i.resolve(&n, &n.links[1]), Ok(Resolved::Note("concepts/b/bfs.md".into())));
    }

    #[test]
    fn paths_and_fragments() {
        let i = from_texts(&[
            ("views/v.md", "# V\n\n## Steps\n\n- one ^abc\n"),
            (
                "journal/r/2025/03/25-03-14.md",
                "# J\n\n[[views/v#steps]] [[views/v#^abc]] [[views/v#nope]] [[../x]] [[/views/v]]\n",
            ),
        ]);
        let n = i.markdown["journal/r/2025/03/25-03-14.md"].clone();
        let r: Vec<_> = n.links.iter().map(|l| i.resolve(&n, l)).collect();
        assert!(r[0].is_ok());
        assert!(r[1].is_ok());
        assert_eq!(r[2], Err(LinkError::Heading));
        assert_eq!(r[3], Err(LinkError::PathForm));
        assert_eq!(r[4], Err(LinkError::PathForm));
    }

    #[test]
    fn embeds_of_files() {
        let i = from_texts(&[
            ("concepts/a/x.md", "---\nid: X\n---\n# X\n\n![[concepts/a/attachments/p.png|300]]\n"),
            ("concepts/a/attachments/p.png", ""),
        ]);
        let (n, l) = first_link(&i, "concepts/a/x.md");
        assert_eq!(i.resolve(&n, &l), Ok(Resolved::File("concepts/a/attachments/p.png".into())));
    }
}
