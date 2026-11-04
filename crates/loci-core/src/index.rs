//! The vault index: every note of a vault, parsed once, plus its concepts' slugs. It
//! resolves what links and note arguments name (m0 §5.3): a slug through its slug
//! table, a path through the vault ([`Vault::resolve`]).

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use crate::envelope::Refusal;
use crate::id::Slug;
use crate::parse::{self, Fragment, Link, Names, Note, Spanned};
use crate::path::{Kind, Place, VaultPath};
use crate::vault::Vault;

#[derive(Debug, Clone)]
pub struct Index<'v> {
    vault: &'v Vault,
    /// Every markdown file of the vault, parsed, by path.
    notes: BTreeMap<VaultPath, Note>,
    /// Concept slug -> concept paths (a concept whose stem is no slug has none).
    slugs: BTreeMap<Slug, Vec<VaultPath>>,
}

/// What a link resolves to: a note of the index, or another vault file.
#[derive(Debug, Clone)]
pub enum Resolved<'a> {
    /// A note, and the bytes of it the link's fragment names (§5.4); `None` for no
    /// fragment.
    Note(&'a Note, Option<Range<usize>>),
    File(&'a VaultPath),
}

impl<'a> Resolved<'a> {
    pub fn path(&self) -> &'a VaultPath {
        match self {
            Resolved::Note(note, _) => &note.path,
            Resolved::File(path) => path,
        }
    }
}

#[derive(Debug, Clone)]
pub enum LinkError<'a> {
    Dangling(Dangling),
    /// The target climbs out of the vault or starts at its root ([`Names::Escaping`]).
    PathForm,
    /// The target `note` has no heading or block that `fragment` names.
    Fragment {
        note: &'a Note,
        fragment: Fragment,
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
#[derive(Debug, Clone)]
pub struct Prereqs<'n> {
    pub resolved: Vec<&'n Note>,
    pub unresolved: Vec<&'n Slug>,
}

impl<'v> Index<'v> {
    /// The index of `vault`'s notes.
    pub fn open(vault: &'v Vault) -> Self {
        let mut index = Index { vault, notes: BTreeMap::new(), slugs: BTreeMap::new() };
        for path in vault.files().iter().filter(|p| p.is_note()) {
            if let Ok(Some(Place::Concept { slug, .. })) = path.place() {
                index.slugs.entry(slug).or_default().push(path.clone());
            }
            index.notes.insert(path.clone(), parse::parse(path, vault.text(path).to_string()));
        }
        index
    }

    /// The vault indexed.
    pub fn vault(&self) -> &'v Vault {
        self.vault
    }

    /// The note a note argument names (m0 §8): one without a `/` whose name, `.md` aside,
    /// is a slug (`bfs`, `bfs.md`) names the concept with that slug, and nothing when no
    /// concept has it; any other (with a `/`, or no slug, such as `AGENTS`) is a written
    /// path, resolved by the vault ([`Vault::resolve`]). Refuses `note.unknown` when it
    /// names no note, and `concept.ambiguous` when its slug names several concepts.
    pub fn note_named(&self, arg: &str) -> Result<&Note, Refusal> {
        let written = crate::path::normalize_arg(arg)?;
        let unknown = || Refusal::refused("note.unknown", format!("no note named `{written}`"));
        let Some(slug) = parse::bare_slug(&written) else {
            return self.vault.resolve(&written).and_then(|p| self.notes.get(p)).ok_or_else(unknown);
        };
        match self.concepts_named(&slug) {
            [] => Err(unknown()),
            [path] => Ok(&self.notes[path]),
            paths => Err(Refusal::refused(
                "concept.ambiguous",
                format!("`{written}` names several concepts ({}); name one by path", paths.join(", ")),
            )),
        }
    }

    /// Each concept slug with the concepts that have it (more than one is
    /// `slug.duplicate`).
    pub fn slugs(&self) -> impl Iterator<Item = (&Slug, &[VaultPath])> {
        self.slugs.iter().map(|(s, ps)| (s, ps.as_slice()))
    }

    /// The concepts whose slug is `slug` (more than one is `slug.duplicate`).
    pub fn concepts_named(&self, slug: &Slug) -> &[VaultPath] {
        self.slugs.get(slug).map_or(&[], Vec::as_slice)
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

    /// The note at vault path `path`, if there is a markdown file there.
    pub fn note_at(&self, path: &VaultPath) -> Option<&Note> {
        self.notes.get(path)
    }

    /// The `prereqs` of concept `note`.
    pub fn prereqs<'n>(&'n self, note: &'n Note) -> Prereqs<'n> {
        let mut prereqs = Prereqs { resolved: vec![], unresolved: vec![] };
        for (link, slug) in note.links.iter().filter_map(|l| Some((l, l.prereq_slug()?))) {
            match self.resolve(note, link) {
                Ok(Resolved::Note(n, _)) => prereqs.resolved.push(n),
                _ => prereqs.unresolved.push(slug),
            }
        }
        prereqs
    }

    /// Area names (m0 §6.3): the folders `concepts/<area>/` that hold a concept or an
    /// `AREA.md`.
    pub fn areas(&self) -> BTreeSet<&str> {
        self.vault
            .files()
            .iter()
            .filter(|f| matches!(f.kind(), Some(Kind::Concept | Kind::Area)))
            .filter_map(VaultPath::area)
            .collect()
    }

    /// Where `link` in note `from` points, ignoring its fragment: a note or file path,
    /// or `None` when it does not resolve.
    pub fn link_target<'a>(&'a self, from: &'a Note, link: &Spanned<Link>) -> Option<&'a VaultPath> {
        self.resolve_target(from, link).ok().map(|r| r.path())
    }

    /// Every link that points at `path`, as (linking note, link), in note path order.
    pub fn incoming(&self, path: &VaultPath) -> impl Iterator<Item = (&Note, &Spanned<Link>)> {
        self.notes.values().flat_map(move |note| {
            let to_path = move |link: &&Spanned<Link>| self.link_target(note, link) == Some(path);
            note.links.iter().filter(to_path).map(move |link| (note, link))
        })
    }

    /// The formed notes ([`Kind::is_formed`]) that mention `target` without linking it,
    /// with the byte range of the first mention, in note path order: the missing links
    /// into `target` (`mention_names` says what a mention is).
    pub fn unlinked_mentions_of(&self, target: &Note) -> Vec<(&Note, Range<usize>)> {
        let names = mention_names(target);
        let linking: BTreeSet<&VaultPath> = self.incoming(&target.path).map(|(n, _)| &n.path).collect();
        self.notes()
            .filter(|n| n.kind.is_formed() && n.path != target.path && !linking.contains(&n.path))
            .filter_map(|n| first_mention(n, &names).map(|at| (n, at)))
            .collect()
    }

    /// The concepts that `note` mentions without linking them, with the byte range of
    /// the first mention in `note`, in text order: the missing links out of `note`. A
    /// mention inside a longer one ("queue" in "priority queue") does not count.
    pub fn unlinked_mentions_in(&self, note: &Note) -> Vec<(&Note, Range<usize>)> {
        let linked: BTreeSet<&VaultPath> = note.links.iter().filter_map(|l| self.link_target(note, l)).collect();
        let mut out: Vec<(&Note, Range<usize>)> = self
            .concepts()
            .filter(|c| c.path != note.path && !linked.contains(&c.path))
            .filter_map(|c| first_mention(note, &mention_names(c)).map(|at| (c, at)))
            .collect();
        let ranges: Vec<Range<usize>> = out.iter().map(|(_, r)| r.clone()).collect();
        out.retain(|(_, r)| !ranges.iter().any(|o| o != r && o.start <= r.start && r.end <= o.end));
        out.sort_by_key(|(c, r)| (r.start, c.path.clone()));
        out
    }

    /// Every markdown note (concepts, area files, views, journal entries, rule files,
    /// markdown sources, inbox drafts), by path.
    pub fn notes(&self) -> impl Iterator<Item = &Note> {
        self.notes.values()
    }

    /// Every concept, by path.
    pub fn concepts(&self) -> impl Iterator<Item = &Note> {
        self.notes.values().filter(|n| n.kind == Kind::Concept)
    }

    /// Resolves a body or `prereqs` link from note `from` (§5.3), then checks its
    /// fragment (§5.4).
    pub fn resolve<'a>(&'a self, from: &'a Note, link: &Spanned<Link>) -> Result<Resolved<'a>, LinkError<'a>> {
        match (self.resolve_target(from, link)?, &link.target.fragment) {
            (Resolved::Note(note, _), Some(frag)) => {
                let range =
                    note.fragment_range(frag).ok_or_else(|| LinkError::Fragment { note, fragment: frag.clone() })?;
                Ok(Resolved::Note(note, Some(range)))
            }
            (resolved, _) => Ok(resolved),
        }
    }

    /// The note or file `link` points at (§5.3), ignoring its fragment.
    fn resolve_target<'a>(&'a self, from: &'a Note, link: &Spanned<Link>) -> Result<Resolved<'a>, LinkError<'a>> {
        match &link.target.names {
            Names::ThisNote => Ok(Resolved::Note(from, None)),
            Names::Slug(slug) => self.resolve_slug(slug),
            Names::Bare(name) => {
                Err(LinkError::Dangling(Dangling::NoConcept { closest: self.closest_slug(name).cloned() }))
            }
            Names::Path(path) => self.resolve_path(path),
            Names::Escaping(_) => Err(LinkError::PathForm),
        }
    }

    /// The concept a slug link names: exactly one concept with that slug.
    fn resolve_slug(&self, slug: &Slug) -> Result<Resolved<'_>, LinkError<'_>> {
        match self.concepts_named(slug) {
            [] => Err(LinkError::Dangling(Dangling::NoConcept { closest: self.closest_slug(slug.as_str()).cloned() })),
            [path] => Ok(Resolved::Note(&self.notes[path], None)),
            paths => Err(LinkError::Dangling(Dangling::Ambiguous(paths.to_vec()))),
        }
    }

    /// The file a path link names ([`Vault::resolve`]).
    fn resolve_path(&self, written: &str) -> Result<Resolved<'_>, LinkError<'_>> {
        let path = self.vault.resolve(written).ok_or(LinkError::Dangling(Dangling::NoFile))?;
        Ok(self.notes.get(path).map_or(Resolved::File(path), |n| Resolved::Note(n, None)))
    }
}

/// ASCII case folded and `-` read as a space, byte for byte, so offsets into the
/// folded text are offsets into the original.
fn fold(s: &str) -> String {
    s.to_ascii_lowercase().replace('-', " ")
}

/// The names a mention of `note` may use, folded: its title, the title without its
/// parenthesised parts, each parenthesised name (`# Breadth-first search (BFS)`), and
/// its file stem read as words (a concept's slug).
fn mention_names(note: &Note) -> Vec<String> {
    let title = note.title();
    let mut names = vec![title.to_string(), note.path.stem().to_string()];
    let (mut bare, mut rest) = (String::new(), &*title);
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
fn first_mention(note: &Note, names: &[String]) -> Option<Range<usize>> {
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
    use crate::testing::vp;

    #[test]
    fn a_bare_link_names_a_concept_by_its_file_name() {
        let i = crate::testing::index(&[
            ("concepts/a/bfs.md", "# BFS\n"),
            ("views/v.md", "# V\n\n[[bfs]] [[bfs.md]] [[bfs.md.md]]\n"),
        ]);
        let n = i.notes["views/v.md"].clone();
        let resolved: Vec<_> = n.links.iter().map(|l| i.link_target(&n, l).map(VaultPath::as_str)).collect();
        assert_eq!(resolved, [Some("concepts/a/bfs.md"), Some("concepts/a/bfs.md"), None]);
    }

    #[test]
    fn path_links_name_notes_and_files_with_their_extension() {
        // A path names the file at exactly that path, else the note with `.md` added.
        let i = crate::testing::index(&[
            ("concepts/a/x.md", "# X\n"),
            ("sources/papers/p.pdf", ""),
            (
                "views/v.md",
                "# V\n\n[[concepts/a/x.md]] [[sources/papers/p.pdf]] [[concepts/a/x.md.md]] [[sources/papers/p]]\n",
            ),
        ]);
        let n = i.notes["views/v.md"].clone();
        let resolved: Vec<_> = n.links.iter().map(|l| i.link_target(&n, l).cloned()).collect();
        assert_eq!(resolved, [Some(vp("concepts/a/x.md")), Some(vp("sources/papers/p.pdf")), None, None]);
    }

    #[test]
    fn a_concept_whose_stem_is_no_slug_has_none() {
        let i = crate::testing::index(&[
            ("concepts/a/dfs.md", "# DFS\n"),
            ("concepts/a/BFS.md", "# BFS\n"),
            ("concepts/a/a_b.md", "# A B\n"),
            ("views/v.md", "# V\n\n[[dfs]] [[DFS]] [[bfs]] [[BFS]] [[a_b]] [[concepts/a/BFS]] [[concepts/a/a_b]]\n"),
        ]);
        let n = i.notes["views/v.md"].clone();
        let resolved: Vec<bool> = n.links.iter().map(|l| i.resolve(&n, l).is_ok()).collect();
        // A slug is written exactly; a misnamed file is reached by path only.
        assert_eq!(resolved, [true, false, false, false, false, true, true]);
        let dfs = Some("dfs".parse().unwrap());
        assert!(
            matches!(i.resolve(&n, &n.links[1]), Err(LinkError::Dangling(Dangling::NoConcept { closest })) if closest == dfs)
        );
    }

    #[test]
    fn a_dangling_slug_suggests_the_closest_concept() {
        let i = crate::testing::index(&[
            ("concepts/a/breadth-first-search.md", "# BFS\n"),
            ("concepts/a/dijkstras-algorithm.md", "# D\n"),
            ("views/v.md", "# V\n\n[[bread-first-search]] [[dijkstra-algo]] [[zebra]]\n"),
        ]);
        let n = i.notes["views/v.md"].clone();
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
        let i = crate::testing::index(&[
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
        let n = i.note_at(&vp("concepts/a/dijkstra.md")).unwrap();
        let found: Vec<(&str, &str)> =
            i.unlinked_mentions_in(n).into_iter().map(|(c, r)| (c.path.stem(), &n.text[r])).collect();
        // "queue" inside "priority queue" is not a mention of Queue; Heap is linked
        // already; "graphs" is another word; Limit is not named.
        assert_eq!(found, [("graph", "graph"), ("priority-queue", "priority queue")]);
    }

    #[test]
    fn unlinked_mentions_are_whole_names_outside_links() {
        let i = crate::testing::index(&[
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
            let n = i.note_at(&vp(p)).unwrap();
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
        let i = crate::testing::index(&[
            ("concepts/a/bfs.md", "# BFS\n"),
            ("views/learn/bfs.md", "# Plan\n\n[[bfs]] [[plan]]\n"),
            ("views/plan.md", "# P\n"),
        ]);
        let n = i.notes["views/learn/bfs.md"].clone();
        assert_eq!(i.link_target(&n, &n.links[0]).cloned(), VaultPath::new("concepts/a/bfs.md".into()));
        assert!(matches!(i.resolve(&n, &n.links[1]), Err(LinkError::Dangling(_))));
    }

    #[test]
    fn a_shared_slug_resolves_to_no_concept() {
        let i = crate::testing::index(&[
            ("concepts/a/bfs.md", "# A\n"),
            ("concepts/b/bfs.md", "# B\n"),
            ("views/v.md", "# V\n\n[[bfs]] [[concepts/b/bfs]]\n"),
        ]);
        let n = i.notes["views/v.md"].clone();
        assert!(matches!(i.resolve(&n, &n.links[0]), Err(LinkError::Dangling(Dangling::Ambiguous(_)))));
        assert_eq!(i.link_target(&n, &n.links[1]).cloned(), VaultPath::new("concepts/b/bfs.md".into()));
    }

    #[test]
    fn paths_and_fragments() {
        let i = crate::testing::index(&[
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
        assert!(
            matches!(&r[2], Err(LinkError::Fragment { fragment: Fragment::Heading(heading), .. }) if heading == "nope"),
            "{:?}",
            r[2]
        );
        assert!(matches!(r[3], Err(LinkError::PathForm)) && matches!(r[4], Err(LinkError::PathForm)));
    }

    #[test]
    fn embeds_of_files() {
        let i = crate::testing::index(&[
            ("concepts/a/x.md", "---\nid: X\n---\n# X\n\n![[concepts/a/attachments/p.png|300]]\n"),
            ("concepts/a/attachments/p.png", ""),
        ]);
        let n = &i.notes["concepts/a/x.md"];
        assert!(
            matches!(i.resolve(n, &n.links[0]), Ok(Resolved::File(p)) if p.as_str() == "concepts/a/attachments/p.png")
        );
    }

    #[test]
    fn a_note_argument_is_a_slug_or_a_path() {
        let i = crate::testing::index(&[
            ("concepts/a/bfs.md", "# BFS\n"),
            ("concepts/b/dup.md", "# D\n"),
            ("concepts/c/dup.md", "# D\n"),
            ("views/v.md", "# V\n"),
            ("AGENTS.md", "# Agents\n"),
            ("assets/x.png", ""),
        ]);
        let resolve = |arg: &str| i.note_named(arg).map(|n| n.path.as_str());
        assert_eq!(resolve("views/v"), Ok("views/v.md"), "`.md` optional");
        assert_eq!(resolve("views/v.md"), Ok("views/v.md"));
        assert_eq!(resolve("bfs"), Ok("concepts/a/bfs.md"), "a slug");
        assert_eq!(resolve("bfs.md"), Ok("concepts/a/bfs.md"), "a concept's file name");
        assert_eq!(resolve("AGENTS"), Ok("AGENTS.md"), "no slug, so a path");
        assert_eq!(resolve("assets/x.png").unwrap_err().code(), "note.unknown", "no note");
        assert_eq!(resolve("nope").unwrap_err().code(), "note.unknown");
        assert_eq!(resolve("dup").unwrap_err().code(), "concept.ambiguous");
    }

    /// Only the note's own headings count (m0 §4.6): one in a block quote or an item is
    /// no title and no link target.
    #[test]
    fn a_nested_heading_is_no_heading_of_the_note() {
        let i = crate::testing::index(&[
            ("views/v.md", "> # Quoted\n\n- ## Item\n\n## Top\n"),
            ("views/w.md", "[[views/v#Quoted]] [[views/v#Item]] [[views/v#Top]]\n"),
        ]);
        let (v, w) = (&i.notes["views/v.md"], &i.notes["views/w.md"]);
        assert_eq!(v.title(), "v");
        let r: Vec<_> = w.links.iter().map(|l| i.resolve(w, l).is_ok()).collect();
        assert_eq!(r, [false, false, true]);
    }

    /// A setext heading's lines read as one (§4.6): a link names it with a space.
    #[test]
    fn a_multi_line_heading_is_named_on_one_line() {
        let i = crate::testing::index(&[
            ("views/v.md", "Title \r\n  more\r\n===\r\n"),
            ("views/w.md", "[[views/v#Title more]] [[views/v#Title  more]]\n"),
        ]);
        let w = &i.notes["views/w.md"];
        let r: Vec<_> = w.links.iter().map(|l| i.resolve(w, l).is_ok()).collect();
        assert_eq!(r, [true, false]);
    }
}
