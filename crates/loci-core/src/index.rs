//! The vault index: every file core reads, parsed once, plus the name tables used for
//! link resolution (m0 §5).

use std::collections::{BTreeMap, BTreeSet};

use crate::envelope::Refusal;
use crate::parse::{self, Link, LinkForm, Note};
use crate::vault::{self, Kind};

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
    /// Every markdown file core reads, parsed, by path.
    markdown: BTreeMap<String, Note>,
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
    build(map.keys().map(|p| p.to_string()).collect(), |p| map[p].to_string())
}

/// Indexes `files` (sorted vault paths), taking each `.md` file's text from `text`.
pub fn build(files: Vec<String>, text: impl Fn(&str) -> String) -> Index {
    let mut index =
        Index { files: files.clone(), markdown: BTreeMap::new(), slugs: BTreeMap::new(), targets: BTreeSet::new() };
    for path in files {
        let kind = vault::kind_of(&path).expect("the index holds only paths core reads");
        if let Some(stripped) = path.strip_suffix(".md") {
            index.targets.insert(stripped.to_string());
            if kind == Kind::Concept {
                index.slugs.entry(vault::stem(&path).to_lowercase()).or_default().push(path.clone());
            }
            let note = parse::parse(&path, kind, text(&path));
            index.markdown.insert(path, note);
        } else {
            index.targets.insert(path);
        }
    }
    index
}

impl Index {
    /// The index of the files listed when `vault` was opened.
    pub fn open(vault: &vault::Vault) -> Self {
        let listing = vault.listing();
        build(listing.files.clone(), |p| listing.markdown[p].clone())
    }

    /// The note a note argument names (m0 §8): a vault path to any note (`.md`
    /// optional), else a concept slug (case-insensitive). A slug shared by several
    /// concepts is ambiguous. `arg` is a [`vault::path_arg`] result.
    pub fn note(&self, arg: &str) -> Result<&Note, ConceptLookup> {
        if let Some(note) = self.markdown.get(&note_path(arg)) {
            return Ok(note);
        }
        // No markdown file at that path: the argument may be a concept slug.
        match self.slugs.get(&arg.to_lowercase()).map(Vec::as_slice) {
            None | Some([]) => Err(ConceptLookup::Unknown),
            Some([p]) => Ok(&self.markdown[p]),
            Some(ps) => Err(ConceptLookup::Ambiguous(ps.to_vec())),
        }
    }

    /// The parsed note at vault path `path`, if there is a markdown file there.
    pub fn parsed(&self, path: &str) -> Option<&Note> {
        self.markdown.get(path)
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
        self.files
            .iter()
            .filter(|f| matches!(vault::kind_of(f), Some(Kind::Concept | Kind::Area)))
            .filter_map(|f| vault::area_of(f))
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

    /// The first folder or file on the way to `path` whose name differs from an existing
    /// one only in letter case, as (existing, asked) (m0 §9.5). A name that exists
    /// exactly is no twin. On a case-insensitive file system (macOS) such a path is the
    /// existing one, and git with `core.ignorecase` keeps the existing spelling; elsewhere
    /// it makes a twin. `new` and `mv` refuse it (`path.case-twin`).
    pub fn case_twin(&self, path: &str) -> Option<(String, String)> {
        let parts: Vec<&str> = path.split('/').collect();
        (1..=parts.len()).find_map(|k| {
            let asked = parts[..k].join("/");
            if self.has_entry(&asked) {
                return None;
            }
            self.files.iter().find_map(|f| {
                let head: Vec<&str> = f.split('/').take(k).collect();
                let existing = head.join("/");
                (head.len() == k && existing != asked && existing.to_lowercase() == asked.to_lowercase())
                    .then(|| (existing, asked.clone()))
            })
        })
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

    /// The existing concept slug most like `slug`, if one is close (Jaro-Winkler
    /// similarity of at least 0.85): a likely typo or a shortened name.
    pub fn closest_slug(&self, slug: &str) -> Option<&str> {
        self.slugs
            .iter()
            .filter(|(_, paths)| !paths.is_empty())
            .map(|(s, _)| (strsim::jaro_winkler(slug, s), s.as_str()))
            .filter(|(score, _)| *score >= 0.85)
            .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(a.1)))
            .map(|(_, s)| s)
    }

    /// The concept paths that have slug `slug` (case-insensitive).
    pub fn slug_paths(&self, slug: &str) -> &[String] {
        self.slugs.get(&slug.to_lowercase()).map_or(&[], Vec::as_slice)
    }

    /// The formed notes ([`Kind::is_formed`]) that mention `target` but do not link
    /// it, with the byte offset of the first mention, in note path order. A mention is
    /// the title, the title without its parenthesised parts, each parenthesised name
    /// (`# Breadth-first search (BFS)`), or the slug read as words. ASCII case is
    /// folded and `-` matches a space; a Latin match must stand as whole words;
    /// mentions inside links are not counted.
    pub fn unlinked_mentions(&self, target: &Note) -> Vec<(&Note, usize)> {
        // Byte for byte, so offsets into the folded text are offsets into the note.
        let fold = |s: &str| s.to_ascii_lowercase().replace('-', " ");
        let title = target.title();
        let mut names = vec![title.to_string(), vault::stem(&target.path).to_string()];
        let (mut bare, mut rest) = (String::new(), title);
        while let Some((before, after)) = rest.split_once('(') {
            bare.push_str(before);
            let Some((inner, tail)) = after.split_once(')') else { break };
            names.push(inner.to_string());
            rest = tail;
        }
        bare.push_str(rest);
        names.push(bare);
        let mut names: Vec<String> = names
            .iter()
            .map(|n| fold(n).split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|n| !n.is_empty())
            .collect();
        names.sort();
        names.dedup();
        let linking: BTreeSet<&str> = self.incoming(&target.path).iter().map(|(n, _)| n.path.as_str()).collect();
        let edge = |c: Option<char>| !c.is_some_and(|c| c.is_ascii_alphanumeric());
        let mut out = vec![];
        for note in self.notes() {
            if !note.kind.is_formed() || note.path == target.path || linking.contains(note.path.as_str()) {
                continue;
            }
            let text = fold(&note.text);
            let start = note.body_start();
            let in_link = |at: usize| note.links.iter().any(|l| l.range.contains(&at));
            let first = names
                .iter()
                .filter_map(|name| {
                    text[start..].match_indices(name.as_str()).map(|(i, _)| start + i).find(|&at| {
                        edge(text[..at].chars().next_back())
                            && edge(text[at + name.len()..].chars().next())
                            && !in_link(at)
                    })
                })
                .min();
            if let Some(at) = first {
                out.push((note, at));
            }
        }
        out
    }

    /// Whether `slug` is already a concept's slug, other than paths `moving` accepts,
    /// which are being renamed (§9.5, §9.7).
    pub fn name_taken(&self, slug: &str, moving: impl Fn(&str) -> bool) -> bool {
        self.slugs.get(&slug.to_lowercase()).is_some_and(|ps| ps.iter().any(|p| !moving(p)))
    }

    /// Every markdown note (concepts, area files, views, journal entries, rule files,
    /// markdown sources, inbox drafts), by path.
    pub fn notes(&self) -> impl Iterator<Item = &Note> {
        self.markdown.values()
    }

    /// Every concept, by path.
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
                    let hint = match self.closest_slug(&key) {
                        Some(s) => format!("no concept is named `{key}`; did you mean [[{s}]]?"),
                        None => format!(
                            "no concept is named `{key}`; create it with `loci new`, or link a non-concept by path"
                        ),
                    };
                    return Err(LinkError::Dangling { hint: Some(hint) });
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
    fn check_fragment(&self, path: &str, frag: &str) -> Result<(), LinkError> {
        let note = &self.markdown[path];
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
    use crate::envelope::line_col;

    fn first_link(i: &Index, path: &str) -> (Note, Link) {
        let n = i.markdown[path].clone();
        let l = n.links[0].clone();
        (n, l)
    }

    #[test]
    fn a_dangling_slug_suggests_the_closest_concept() {
        let i = from_texts(&[
            ("concepts/a/breadth-first-search.md", "# BFS\n"),
            ("concepts/a/dijkstras-algorithm.md", "# D\n"),
            ("views/v.md", "# V\n\n[[bread-first-search]] [[dijkstra-algo]] [[zebra]]\n"),
        ]);
        let n = i.markdown["views/v.md"].clone();
        let hint = |k: usize| match i.resolve(&n, &n.links[k]) {
            Err(LinkError::Dangling { hint: Some(h) }) => h,
            other => panic!("{other:?}"),
        };
        assert!(hint(0).ends_with("did you mean [[breadth-first-search]]?"), "{}", hint(0));
        assert!(hint(1).ends_with("did you mean [[dijkstras-algorithm]]?"), "{}", hint(1));
        assert!(hint(2).contains("create it with `loci new`"), "{}", hint(2));
    }

    #[test]
    fn unlinked_mentions_are_whole_names_outside_links() {
        let i = from_texts(&[
            ("concepts/a/breadth-first-search.md", "# Breadth-first search (BFS)\n"),
            ("concepts/a/graph.md", "# Graph\n\nA paragraph. Breadth first search walks it.\n"),
            ("concepts/a/queue.md", "# Queue\n\nUsed by [[breadth-first-search]]; BFS again.\n"),
            ("concepts/a/heap.md", "# Heap\n\nNot [[graph|bfs here]] but BFS-like order.\n"),
            ("concepts/a/tree.md", "# Tree\n\nsubfs and bfsx are other words.\n"),
            ("concepts/b/二分.md", "# 二分查找\n"),
            ("views/v.md", "# V\n\n学习二分查找。\n"),
            ("inbox/drafts/d.md", "# D\n\nBFS in a draft.\n"),
            ("sources/docs/s.md", "# S\n\nBFS in a source.\n"),
        ]);
        let found = |p: &str| -> Vec<(String, usize)> {
            let n = i.parsed(p).unwrap();
            i.unlinked_mentions(n).into_iter().map(|(n, at)| (n.path.clone(), line_col(&n.text, at).0)).collect()
        };
        // Hyphen or space, the alias, case; not inside a link, not in a note that links
        // it, not inside another word, not in drafts or sources.
        assert_eq!(
            found("concepts/a/breadth-first-search.md"),
            [("concepts/a/graph.md".to_string(), 3), ("concepts/a/heap.md".to_string(), 3)]
        );
        // A CJK title matches inside running text.
        assert_eq!(found("concepts/b/二分.md"), [("views/v.md".to_string(), 3)]);
        // "graph" inside "paragraph" is no mention.
        assert!(found("concepts/a/graph.md").is_empty());
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
