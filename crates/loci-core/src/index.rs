//! The vault index: every file core reads, parsed once, plus the name tables used for
//! link resolution (m0 §5).

use std::collections::{BTreeMap, BTreeSet};

use crate::envelope::Refusal;
use crate::id::Slug;
use crate::parse::{self, Fragment, Link, Names, Note, Spanned};
use crate::path::Kind;
use crate::path::VaultPath;
use crate::vault;

#[derive(Debug, Clone)]
pub struct Index {
    /// Every file core reads, sorted.
    files: Vec<VaultPath>,
    /// Every markdown file core reads, parsed, by path.
    markdown: BTreeMap<VaultPath, Note>,
    /// Concept slug -> concept paths (a concept whose stem is no slug has none).
    slugs: BTreeMap<Slug, Vec<VaultPath>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// A note, by path.
    Note(VaultPath),
    /// A non-markdown file, by path.
    File(VaultPath),
}

impl Resolved {
    pub fn path(&self) -> &VaultPath {
        match self {
            Resolved::Note(p) | Resolved::File(p) => p,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    Dangling(Dangling),
    PathForm,
    /// The target `note` has no heading `heading`.
    Heading {
        note: VaultPath,
        heading: String,
    },
    /// The target `note` has no block with id `id`.
    Block {
        note: VaultPath,
        id: String,
    },
}

/// Why a link has no target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dangling {
    /// A slug shared by several concepts (`slug.duplicate`), which names none of them.
    Ambiguous(Vec<VaultPath>),
    /// No concept has the name; `closest` is the slug it most likely meant.
    NoConcept { closest: Option<Slug> },
    /// No file at the path.
    NoFile,
}

/// A concept's `prereqs` (§6.1): the notes they name, and the slugs that name none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prereqs<'n> {
    pub resolved: Vec<VaultPath>,
    pub unresolved: Vec<&'n Slug>,
}

impl Index {
    /// An index of in-memory files: `.md` texts are parsed, other contents are unused.
    pub fn from_texts(files: &[(impl AsRef<str>, impl AsRef<str>)]) -> Index {
        let map: BTreeMap<VaultPath, &str> = files
            .iter()
            .map(|(p, t)| {
                let p = VaultPath::new(p.as_ref().to_string()).expect("`from_texts` is given vault paths");
                (p, t.as_ref())
            })
            .collect();
        Self::build(map.keys().cloned().collect(), |p| map[p].to_string())
    }

    /// Indexes `files` (sorted vault paths), taking each `.md` file's text from `text`.
    pub fn build(files: Vec<VaultPath>, text: impl Fn(&VaultPath) -> String) -> Index {
        let mut index = Index { files: files.clone(), markdown: BTreeMap::new(), slugs: BTreeMap::new() };
        for path in files {
            let kind = path.kind().expect("the index holds only paths core reads");
            if path.is_note() {
                if let Some(slug) = path.slug().filter(|_| kind == Kind::Concept) {
                    index.slugs.entry(slug).or_default().push(path.clone());
                }
                let note = parse::parse(&path, kind, text(&path));
                index.markdown.insert(path, note);
            }
        }
        index
    }

    /// The index of the files listed when `vault` was opened.
    pub fn open(vault: &vault::Vault) -> Self {
        let listing = vault.listing();
        Self::build(listing.files.clone(), |p| listing.markdown[p].clone())
    }

    /// The note a note argument names (m0 §8): a vault path to any note (`.md`
    /// optional), else a concept slug, written exactly. Refused with `unknown_code`
    /// when none matches, and `concept.ambiguous` when a slug names several concepts.
    pub fn note(&self, arg: &VaultPath, unknown_code: &str) -> Result<&Note, Refusal> {
        if let Some(note) = self.markdown.get(&arg.as_note()) {
            return Ok(note);
        }
        // No markdown file at that path: the argument may be a concept slug.
        match self.slugs.get(arg.as_str()).map(Vec::as_slice) {
            None | Some([]) => Err(Refusal::refused(unknown_code, format!("no note named `{arg}`"))),
            Some([p]) => Ok(&self.markdown[p]),
            Some(ps) => Err(Refusal::refused(
                "concept.ambiguous",
                format!("`{arg}` names several concepts ({}); name one by path", ps.join(", ")),
            )),
        }
    }

    /// Every file core reads, sorted.
    pub fn files(&self) -> &[VaultPath] {
        &self.files
    }

    /// Each concept slug with the concepts that have it (more than one is
    /// `slug.duplicate`).
    pub fn slugs(&self) -> impl Iterator<Item = (&Slug, &[VaultPath])> {
        self.slugs.iter().map(|(s, ps)| (s, ps.as_slice()))
    }

    /// The parsed note at vault path `path`, if there is a markdown file there.
    pub fn parsed(&self, path: &VaultPath) -> Option<&Note> {
        self.markdown.get(path)
    }

    /// The concept a note argument names: [`Index::note`], refused with `unknown_code`
    /// too when that note is not a concept.
    pub fn concept(&self, arg: &VaultPath, unknown_code: &str) -> Result<&Note, Refusal> {
        let note = self.note(arg, unknown_code)?;
        match note.kind {
            Kind::Concept => Ok(note),
            _ => Err(Refusal::refused(unknown_code, format!("`{}` is not a concept", note.path))),
        }
    }

    /// The `prereqs` of concept `note`.
    pub fn prereqs<'n>(&self, note: &'n Note) -> Prereqs<'n> {
        let mut prereqs = Prereqs { resolved: vec![], unresolved: vec![] };
        for (link, slug) in note.links.iter().filter_map(|l| Some((l, l.prereq_slug()?))) {
            match self.resolve(note, link) {
                Ok(Resolved::Note(p)) => prereqs.resolved.push(p),
                _ => prereqs.unresolved.push(slug),
            }
        }
        prereqs
    }

    /// Area names (m0 §6.3): the folders `concepts/<area>/` that hold a concept or an
    /// `AREA.md`.
    pub fn areas(&self) -> BTreeSet<&str> {
        self.files
            .iter()
            .filter(|f| matches!(f.kind(), Some(Kind::Concept | Kind::Area)))
            .filter_map(VaultPath::area)
            .collect()
    }

    /// Where `link` in note `from` points, ignoring its fragment: a note or file path,
    /// or `None` when it does not resolve.
    pub fn link_target(&self, from: &Note, link: &Spanned<Link>) -> Option<VaultPath> {
        self.resolve_target(from, link).ok().map(|r| r.path().clone())
    }

    /// Whether the vault has a file at `path`, or a folder (files under `path/`).
    pub fn has_entry(&self, path: &VaultPath) -> bool {
        self.files.iter().any(|f| f == path || f.starts_with_folder(path))
    }

    /// Whether `path` is a file the index holds.
    pub fn contains(&self, path: &VaultPath) -> bool {
        self.files.binary_search(path).is_ok()
    }

    /// The listed file at `path`, if any.
    fn listed(&self, path: &str) -> Option<&VaultPath> {
        self.files.binary_search_by(|f| f.as_str().cmp(path)).ok().map(|i| &self.files[i])
    }

    /// The first folder or file on the way to `path` whose name differs from an existing
    /// one only in letter case, as (existing, asked) (m0 §9.5). A name that exists
    /// exactly is no twin. On a case-insensitive file system (macOS) such a path is the
    /// existing one, and git with `core.ignorecase` keeps the existing spelling; elsewhere
    /// it makes a twin. `new` and `mv` refuse it (`path.case-twin`).
    pub fn case_twin(&self, path: &VaultPath) -> Option<(VaultPath, VaultPath)> {
        (1..=path.components().count()).find_map(|k| {
            let asked = path.head(k).expect("`k` is at most the number of segments");
            if self.has_entry(&asked) {
                return None;
            }
            self.files.iter().find_map(|f| {
                let existing = f.head(k)?;
                (existing != asked && existing.as_str().to_lowercase() == asked.as_str().to_lowercase())
                    .then(|| (existing, asked.clone()))
            })
        })
    }

    /// Every link that points at `path`, as (linking note, link), in note path order.
    pub fn incoming(&self, path: &VaultPath) -> Vec<(&Note, &Spanned<Link>)> {
        let mut out = vec![];
        for note in self.markdown.values() {
            for link in &note.links {
                let Some(target) = self.link_target(note, link) else { continue };
                if target == *path {
                    out.push((note, link));
                }
            }
        }
        out
    }

    /// The existing concept slug most like `name`, if one is close (Jaro-Winkler
    /// similarity of at least 0.85, letter case aside): a likely typo, a shortened name
    /// or a capitalised one.
    fn closest_slug(&self, name: &str) -> Option<&Slug> {
        let name = name.to_lowercase();
        self.slugs
            .keys()
            .map(|s| (strsim::jaro_winkler(&name, s.as_str()), s))
            .filter(|(score, _)| *score >= 0.85)
            .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(a.1)))
            .map(|(_, s)| s)
    }

    /// The concept paths that have slug `slug`.
    pub fn slug_paths(&self, slug: &Slug) -> &[VaultPath] {
        self.slugs.get(slug).map_or(&[], Vec::as_slice)
    }

    /// The formed notes ([`Kind::is_formed`]) that mention `target` without linking it,
    /// with the byte range of the first mention, in note path order: the missing links
    /// into `target` (`mention_names` says what a mention is).
    pub fn unlinked_mentions_of(&self, target: &Note) -> Vec<(&Note, std::ops::Range<usize>)> {
        let names = mention_names(target);
        let linking: BTreeSet<&VaultPath> = self.incoming(&target.path).iter().map(|(n, _)| &n.path).collect();
        self.notes()
            .filter(|n| n.kind.is_formed() && n.path != target.path && !linking.contains(&n.path))
            .filter_map(|n| first_mention(n, &names).map(|at| (n, at)))
            .collect()
    }

    /// The concepts that `note` mentions without linking them, with the byte range of
    /// the first mention in `note`, in text order: the missing links out of `note`. A
    /// mention inside a longer one ("queue" in "priority queue") does not count.
    pub fn unlinked_mentions_in(&self, note: &Note) -> Vec<(&Note, std::ops::Range<usize>)> {
        let linked: BTreeSet<VaultPath> = note.links.iter().filter_map(|l| self.link_target(note, l)).collect();
        let mut out: Vec<(&Note, std::ops::Range<usize>)> = self
            .concepts()
            .filter(|c| c.path != note.path && !linked.contains(&c.path))
            .filter_map(|c| first_mention(note, &mention_names(c)).map(|at| (c, at)))
            .collect();
        let ranges: Vec<std::ops::Range<usize>> = out.iter().map(|(_, r)| r.clone()).collect();
        out.retain(|(_, r)| !ranges.iter().any(|o| o != r && o.start <= r.start && r.end <= o.end));
        out.sort_by_key(|(c, r)| (r.start, c.path.clone()));
        out
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
    pub fn resolve(&self, from: &Note, link: &Spanned<Link>) -> Result<Resolved, LinkError> {
        let resolved = self.resolve_target(from, link)?;
        if let (Some(frag), Resolved::Note(path)) = (&link.target.fragment, &resolved) {
            self.check_fragment(path, frag)?;
        }
        Ok(resolved)
    }

    /// The note or file `link` points at (§5.3), ignoring its fragment.
    fn resolve_target(&self, from: &Note, link: &Spanned<Link>) -> Result<Resolved, LinkError> {
        match &link.target.names {
            Names::ThisNote => Ok(Resolved::Note(from.path.clone())),
            Names::Slug(slug) => self.resolve_slug(slug),
            Names::Bare(name) => {
                Err(LinkError::Dangling(Dangling::NoConcept { closest: self.closest_slug(name).cloned() }))
            }
            Names::Path(path) => self.resolve_path(path),
            Names::Escaping(_) => Err(LinkError::PathForm),
        }
    }

    /// The concept a bare link names: exactly one concept with that slug.
    fn resolve_slug(&self, slug: &Slug) -> Result<Resolved, LinkError> {
        match self.slugs.get(slug).map(Vec::as_slice) {
            Some([path]) => Ok(Resolved::Note(path.clone())),
            Some(paths @ [_, _, ..]) => Err(LinkError::Dangling(Dangling::Ambiguous(paths.to_vec()))),
            Some([]) | None => {
                Err(LinkError::Dangling(Dangling::NoConcept { closest: self.closest_slug(slug.as_str()).cloned() }))
            }
        }
    }

    /// The file a path link names: a note without `.md`, any other file with its
    /// extension.
    fn resolve_path(&self, path: &str) -> Result<Resolved, LinkError> {
        if let Some(note) = self.listed(&format!("{path}.md")) {
            return Ok(Resolved::Note(note.clone()));
        }
        match self.listed(path).filter(|_| !path.ends_with(".md")) {
            Some(file) => Ok(Resolved::File(file.clone())),
            None => Err(LinkError::Dangling(Dangling::NoFile)),
        }
    }

    /// Checks a `#fragment` against the headings or block ids of note `path` (§5.4).
    fn check_fragment(&self, path: &VaultPath, frag: &Fragment) -> Result<(), LinkError> {
        match (self.markdown[path].fragment_range(frag), frag) {
            (Some(_), _) => Ok(()),
            (None, Fragment::Heading(heading)) => {
                Err(LinkError::Heading { note: path.clone(), heading: heading.clone() })
            }
            (None, Fragment::Block(id)) => Err(LinkError::Block { note: path.clone(), id: id.clone() }),
        }
    }
}

/// ASCII case folded and `-` read as a space, byte for byte, so offsets into the
/// folded text are offsets into the original.
fn fold(s: &str) -> String {
    s.to_ascii_lowercase().replace('-', " ")
}

/// The names a mention of `note` may use, folded: its title, the title without its
/// parenthesised parts, each parenthesised name (`# Breadth-first search (BFS)`), and
/// its slug read as words.
fn mention_names(note: &Note) -> Vec<String> {
    let title = note.title();
    let mut names = vec![title.to_string(), note.path.stem().to_string()];
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
    names
}

/// The byte range of the first mention of any of `names` in the body of `note`: a
/// Latin match stands as whole words, and text inside a link does not count.
fn first_mention(note: &Note, names: &[String]) -> Option<std::ops::Range<usize>> {
    let edge = |c: Option<char>| !c.is_some_and(|c| c.is_ascii_alphanumeric());
    let text = fold(&note.text);
    let start = note.body_start();
    let in_link = |at: usize| note.links.iter().any(|l| l.span.clone().contains(&at));
    names
        .iter()
        .filter_map(|name| {
            text[start..]
                .match_indices(name.as_str())
                .map(|(i, _)| start + i)
                .find(|&at| {
                    edge(text[..at].chars().next_back()) && edge(text[at + name.len()..].chars().next()) && !in_link(at)
                })
                .map(|at| at..at + name.len())
        })
        .min_by_key(|r| (r.start, std::cmp::Reverse(r.end)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::line_col;

    fn first_link(i: &Index, path: &str) -> (Note, Spanned<Link>) {
        let n = i.markdown[path].clone();
        let l = n.links[0].clone();
        (n, l)
    }

    #[test]
    fn path_links_name_notes_and_files_with_their_extension() {
        // `.md` is optional for a note; a target still ending in `.md` names no file.
        let i = Index::from_texts(&[
            ("concepts/a/x.md", "# X\n"),
            ("sources/papers/p.pdf", ""),
            (
                "views/v.md",
                "# V\n\n[[concepts/a/x.md]] [[sources/papers/p.pdf]] [[concepts/a/x.md.md]] [[sources/papers/p]]\n",
            ),
        ]);
        let n = i.markdown["views/v.md"].clone();
        let resolved: Vec<_> = n.links.iter().map(|l| i.resolve(&n, l).ok()).collect();
        let vp = |s: &str| VaultPath::new(s.into()).unwrap();
        assert_eq!(
            resolved,
            [Some(Resolved::Note(vp("concepts/a/x.md"))), Some(Resolved::File(vp("sources/papers/p.pdf"))), None, None]
        );
    }

    #[test]
    fn a_concept_whose_stem_is_no_slug_has_none() {
        let i = Index::from_texts(&[
            ("concepts/a/dfs.md", "# DFS\n"),
            ("concepts/a/BFS.md", "# BFS\n"),
            ("concepts/a/a_b.md", "# A B\n"),
            ("views/v.md", "# V\n\n[[dfs]] [[DFS]] [[bfs]] [[BFS]] [[a_b]] [[concepts/a/BFS]] [[concepts/a/a_b]]\n"),
        ]);
        let n = i.markdown["views/v.md"].clone();
        let resolved: Vec<bool> = n.links.iter().map(|l| i.resolve(&n, l).is_ok()).collect();
        // A slug is written exactly; a misnamed file is reached by path only.
        assert_eq!(resolved, [true, false, false, false, false, true, true]);
        let dfs = Some("dfs".parse().unwrap());
        assert_eq!(i.resolve(&n, &n.links[1]), Err(LinkError::Dangling(Dangling::NoConcept { closest: dfs })));
    }

    #[test]
    fn a_dangling_slug_suggests_the_closest_concept() {
        let i = Index::from_texts(&[
            ("concepts/a/breadth-first-search.md", "# BFS\n"),
            ("concepts/a/dijkstras-algorithm.md", "# D\n"),
            ("views/v.md", "# V\n\n[[bread-first-search]] [[dijkstra-algo]] [[zebra]]\n"),
        ]);
        let n = i.markdown["views/v.md"].clone();
        let closest = |k: usize| match i.resolve(&n, &n.links[k]) {
            Err(LinkError::Dangling(Dangling::NoConcept { closest })) => closest.map(|s| s.to_string()),
            other => panic!("{other:?}"),
        };
        assert_eq!(closest(0).as_deref(), Some("breadth-first-search"));
        assert_eq!(closest(1).as_deref(), Some("dijkstras-algorithm"));
        assert_eq!(closest(2), None);
    }

    #[test]
    fn unlinked_mentions_in_a_note_are_the_concepts_it_names() {
        let i = Index::from_texts(&[
            ("concepts/a/queue.md", "# Queue\n"),
            ("concepts/a/priority-queue.md", "# Priority queue\n"),
            ("concepts/a/graph.md", "# Graph\n"),
            ("concepts/a/heap.md", "# Heap\n"),
            ("concepts/a/limit.md", "# Limit\n"),
            (
                "concepts/a/dijkstra.md",
                "# Dijkstra\n\nOn a graph, take from a priority queue; see [[heap]] and the heap. No graphs here.\n",
            ),
        ]);
        let n = i.parsed(&VaultPath::new("concepts/a/dijkstra.md".into()).unwrap()).unwrap();
        let found: Vec<(&str, &str)> =
            i.unlinked_mentions_in(n).into_iter().map(|(c, r)| (c.path.stem(), &n.text[r])).collect();
        // "queue" inside "priority queue" is not a mention of Queue; Heap is linked
        // already; "graphs" is another word; Limit is not named.
        assert_eq!(found, [("graph", "graph"), ("priority-queue", "priority queue")]);
    }

    #[test]
    fn unlinked_mentions_are_whole_names_outside_links() {
        let i = Index::from_texts(&[
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
            let n = i.parsed(&VaultPath::new(p.into()).unwrap()).unwrap();
            i.unlinked_mentions_of(n)
                .into_iter()
                .map(|(n, r)| (n.path.to_string(), line_col(&n.text, r.start).0))
                .collect()
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
        let i = Index::from_texts(&[
            ("concepts/a/bfs.md", "# BFS\n"),
            ("views/learn/bfs.md", "# Plan\n\n[[bfs]] [[plan]]\n"),
            ("views/plan.md", "# P\n"),
        ]);
        let n = i.markdown["views/learn/bfs.md"].clone();
        assert_eq!(i.resolve(&n, &n.links[0]), Ok(Resolved::Note(VaultPath::new("concepts/a/bfs.md".into()).unwrap())));
        assert!(matches!(i.resolve(&n, &n.links[1]), Err(LinkError::Dangling(_))));
    }

    #[test]
    fn a_shared_slug_resolves_to_no_concept() {
        let i = Index::from_texts(&[
            ("concepts/a/bfs.md", "# A\n"),
            ("concepts/b/bfs.md", "# B\n"),
            ("views/v.md", "# V\n\n[[bfs]] [[concepts/b/bfs]]\n"),
        ]);
        let n = i.markdown["views/v.md"].clone();
        assert!(matches!(i.resolve(&n, &n.links[0]), Err(LinkError::Dangling(Dangling::Ambiguous(_)))));
        assert_eq!(i.resolve(&n, &n.links[1]), Ok(Resolved::Note(VaultPath::new("concepts/b/bfs.md".into()).unwrap())));
    }

    #[test]
    fn paths_and_fragments() {
        let i = Index::from_texts(&[
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
        assert!(matches!(&r[2], Err(LinkError::Heading { heading, .. }) if heading == "nope"), "{:?}", r[2]);
        assert_eq!(r[3], Err(LinkError::PathForm));
        assert_eq!(r[4], Err(LinkError::PathForm));
    }

    #[test]
    fn embeds_of_files() {
        let i = Index::from_texts(&[
            ("concepts/a/x.md", "---\nid: X\n---\n# X\n\n![[concepts/a/attachments/p.png|300]]\n"),
            ("concepts/a/attachments/p.png", ""),
        ]);
        let (n, l) = first_link(&i, "concepts/a/x.md");
        assert_eq!(
            i.resolve(&n, &l),
            Ok(Resolved::File(VaultPath::new("concepts/a/attachments/p.png".into()).unwrap()))
        );
    }
}
