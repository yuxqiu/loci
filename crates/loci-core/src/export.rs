//! `loci export` (m1-m4 §4): assemble one self-contained markdown document from a view
//! or concept and the notes it links to.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Range;

use crate::index::{Index, Resolved, heading_key};
use crate::parse::{Fragment, Heading, Link, LinkForm, Note, Spanned};
use crate::path::VaultPath;
use crate::splice::{self, Edit};

/// An included note, with how its content is placed in the document.
struct Unit<'a> {
    note: &'a Note,
    /// The included bytes of the note.
    range: Range<usize>,
    /// Added to the level of each heading in `range`.
    shift: i32,
    /// A heading inserted before the content (level, text), when the content does not
    /// start with a heading.
    title: Option<(u8, String)>,
}

impl<'a> Unit<'a> {
    fn root(note: &'a Note) -> Self {
        Unit { note, range: note.body_start()..note.text.len(), shift: 0, title: None }
    }

    /// The unit for `part` of `note` (m1-m4 §4 step 1: the whole note, the section under
    /// a heading, or one block), placed under a heading of `level`. `resolve` has checked
    /// that the fragment exists.
    fn child(note: &'a Note, part: Option<&Fragment>, level: u8) -> Self {
        let range = match part {
            None => note.body_start()..note.text.len(),
            Some(Fragment::Heading(f)) => section_range(note, f).expect("resolve checked the heading"),
            Some(Fragment::Block(id)) => {
                note.blocks.iter().find(|b| *b.id == *id).expect("resolve checked the block id").span.clone()
            }
        };
        let headings: Vec<&Spanned<Heading>> = headings_in(note, &range).collect();
        match headings.first() {
            Some(h) if h.span.start == content_start(note, &range) => {
                Unit { note, range, shift: level as i32 - h.level as i32, title: None }
            }
            _ => {
                let min = headings.iter().map(|h| h.level).min();
                let shift = min.map_or(0, |m| level as i32 + 1 - m as i32);
                Unit { note, range, shift, title: Some((level, note.title().to_string())) }
            }
        }
    }

    fn level(&self, h: &Spanned<Heading>) -> u8 {
        (h.level as i32 + self.shift).clamp(1, 6) as u8
    }

    /// The output level of the nearest heading before byte `at` (0 if none).
    fn level_at(&self, at: usize) -> u8 {
        match headings_in(self.note, &self.range).rfind(|h| h.span.start < at) {
            Some(h) => self.level(h),
            None => self.title.as_ref().map_or(0, |(l, _)| *l),
        }
    }
}

/// The headings of `note` inside `range`.
fn headings_in<'n>(note: &'n Note, range: &Range<usize>) -> impl DoubleEndedIterator<Item = &'n Spanned<Heading>> {
    let range = range.clone();
    note.headings.iter().filter(move |h| range.start <= h.span.start && h.span.end <= range.end)
}

/// The first non-whitespace byte of `range` in `note`.
fn content_start(note: &Note, range: &Range<usize>) -> usize {
    let text = &note.text[range.clone()];
    range.start + (text.len() - text.trim_start().len())
}

/// Anchors of one unit in the assembled document.
#[derive(Default)]
struct Anchors {
    /// The unit's top heading (its inserted title, or the heading it starts with).
    top: Option<String>,
    /// By heading key (§5.4), the first heading with that key.
    headings: BTreeMap<String, String>,
}

/// The identifiers of the headings of the units: per unit, its anchors; and every
/// identifier in document order. Each is [`anchor_id`] of the heading text, with `-1`,
/// `-2`, … appended when it is already taken, as GitHub does.
fn anchors(units: &[Unit]) -> (Vec<Anchors>, Vec<String>) {
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut order: Vec<String> = vec![];
    let mut unique = |text: &str| {
        let base = anchor_id(text);
        let id = (0..).map(|n| if n == 0 { base.clone() } else { format!("{base}-{n}") }).find(|c| !used.contains(c));
        let id = id.expect("an unused identifier exists");
        used.insert(id.clone());
        order.push(id.clone());
        id
    };

    let mut out = vec![];
    for u in units {
        let mut a = Anchors::default();
        if let Some((_, title)) = &u.title {
            a.top = Some(unique(title));
        }
        let start = content_start(u.note, &u.range);
        for h in headings_in(u.note, &u.range) {
            let id = unique(&h.content);
            if a.top.is_none() && h.span.start == start {
                a.top = Some(id.clone());
            }
            a.headings.entry(heading_key(&h.content)).or_insert(id);
        }
        out.push(a);
    }

    drop(unique);
    (out, order)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Export {
    pub markdown: String,
    /// The identifier of every heading of `markdown`, in document order: what its
    /// `#anchor` links point at, for a renderer to give the headings.
    pub heading_ids: Vec<String>,
    /// Included note paths, in order.
    pub notes: Vec<VaultPath>,
    /// Images to copy: (vault path, file name under `assets/`).
    pub assets: Vec<(VaultPath, String)>,
}

/// The identifier of a heading text, by GitHub's rule, so that the links of a markdown
/// export also work where GitHub renders it: lowercase; letters, digits, `-` and `_`
/// kept; each space a `-`; everything else dropped.
fn anchor_id(text: &str) -> String {
    let s: String = text
        .chars()
        .filter_map(|c| match c {
            c if c.is_alphanumeric() || c == '_' || c == '-' => Some(c.to_lowercase().collect::<String>()),
            ' ' => Some("-".to_string()),
            _ => None,
        })
        .collect();
    if s.is_empty() { "section".to_string() } else { s }
}

fn is_image(path: &VaultPath) -> bool {
    let lower = path.as_str().to_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".bmp"].iter().any(|e| lower.ends_with(e))
}

/// The section under the heading `frag` names in `note` ([`Note::section`]).
fn section_range(note: &Note, frag: &str) -> Option<Range<usize>> {
    let key = heading_key(frag);
    let i = note.headings.iter().position(|h| heading_key(&h.content) == key)?;
    Some(note.section(i))
}

/// The note links of `note` within `range`, resolved, in order.
fn note_links<'a>(index: &Index, note: &'a Note, range: &Range<usize>) -> Vec<(&'a Spanned<Link>, Resolved)> {
    note.links
        .iter()
        .filter(|l| matches!(l.form, LinkForm::Wiki | LinkForm::Embed))
        .filter(|l| range.start <= l.span.start && l.span.end <= range.end)
        .filter_map(|l| index.resolve(note, l).ok().map(|r| (l, r)))
        .collect()
}

pub fn export(index: &Index, root: &VaultPath, depth: usize) -> Export {
    // 1. Collect, then name every heading once.
    let (units, unit_of) = collect(index, root, depth);
    let (anchors, heading_ids) = anchors(&units);

    // 2–4. Assemble, rewriting headings, links, images and block ids.
    let mut assets = Assets::default();
    let mut doc = String::new();
    let mut notes = vec![];
    for u in &units {
        let note = u.note;
        let mut edits: Vec<Edit> = vec![];
        for h in headings_in(note, &u.range) {
            edits.extend(heading_edits(&note.text, h, u.level(h)));
        }
        // Block ids are vault plumbing: dropped, with the spaces before them.
        for id in
            note.blocks.iter().map(|b| &b.id).filter(|id| u.range.start <= id.span.start && id.span.end <= u.range.end)
        {
            let start = note.text[..id.span.start].trim_end_matches([' ', '\t']).len();
            edits.push(Edit::replace(start..id.span.end, ""));
        }
        for (link, resolved) in note_links(index, note, &u.range) {
            let new = link_replacement(index, note, link, resolved, &unit_of, &anchors, &mut assets);
            edits.push(Edit::replace(link.span.clone(), new));
        }

        // Edits are relative to the whole note; cut the unit's range afterwards.
        let edited = splice::apply(&note.text[u.range.clone()], shift_edits(edits, u.range.start))
            .expect("heading, block id and link edits never overlap");
        if !doc.is_empty() && !doc.ends_with("\n\n") {
            doc.push_str(if doc.ends_with('\n') { "\n" } else { "\n\n" });
        }
        if let Some((level, title)) = &u.title {
            doc.push_str(&format!("{} {title}\n\n", "#".repeat(*level as usize)));
        }
        doc.push_str(edited.trim_start_matches('\n'));
        notes.push(note.path.clone());
    }

    Export { markdown: format!("{}\n", doc.trim_end()), notes, assets: assets.names.into_iter().collect(), heading_ids }
}

/// Step 1 (m1-m4 §4): each note once, at its first appearance, breadth-first from
/// `root` up to `depth` links away; and each note's unit index.
fn collect<'a>(index: &'a Index, root: &'a VaultPath, depth: usize) -> (Vec<Unit<'a>>, BTreeMap<&'a VaultPath, usize>) {
    let mut units = vec![Unit::root(index.parsed(root).expect("the command resolved `root` to a readable note"))];
    let mut unit_of: BTreeMap<&VaultPath, usize> = BTreeMap::from([(root, 0)]);
    let mut queue = VecDeque::from([(0usize, 0usize)]);
    while let Some((i, d)) = queue.pop_front() {
        if d >= depth {
            continue;
        }
        let parent = &units[i];
        let mut children = vec![];
        for (link, resolved) in note_links(index, parent.note, &parent.range) {
            let Resolved::Note(p) = resolved else { continue };
            let note = index.parsed(&p).expect("a resolved note");
            let path = &note.path;
            if unit_of.contains_key(path) || children.iter().any(|(q, _): &(&VaultPath, Unit)| *q == path) {
                continue;
            }
            let level = (parent.level_at(link.span.start) + 1).min(6);
            children.push((path, Unit::child(note, link.target.fragment.as_ref(), level)));
        }
        for (path, unit) in children {
            unit_of.insert(path, units.len());
            queue.push_back((units.len(), d + 1));
            units.push(unit);
        }
    }

    (units, unit_of)
}

/// What `link` in `note` becomes in the document: an image under `assets/`, a link to
/// an included note's anchor, or plain text (its label, heading or title) for anything
/// left out.
fn link_replacement(
    index: &Index,
    note: &Note,
    link: &Spanned<Link>,
    resolved: Resolved,
    unit_of: &BTreeMap<&VaultPath, usize>,
    anchors: &[Anchors],
    assets: &mut Assets,
) -> String {
    let label = link.label.as_ref().map(|l| l.text(&note.text).to_string());
    let p = match resolved {
        Resolved::File(p) if link.form == LinkForm::Embed && is_image(&p) => {
            let name = assets.name(&p);
            let alt = label.filter(|l| !is_size(l)).unwrap_or_else(|| name.clone());
            return format!("![{alt}]({ASSETS}/{name})");
        }
        Resolved::File(p) => return label.unwrap_or_else(|| p.to_string()),
        Resolved::Note(p) => p,
    };

    let heading = match &link.target.fragment {
        Some(Fragment::Heading(h)) => Some(h.as_str()),
        Some(Fragment::Block(_)) | None => None,
    };

    let text =
        label.unwrap_or_else(|| heading.unwrap_or(index.parsed(&p).expect("a resolved note").title()).to_string());
    let anchor = unit_of.get(&p).and_then(|&j| {
        let a = &anchors[j];
        heading.and_then(|h| a.headings.get(&heading_key(h))).or(a.top.as_ref())
    });

    match anchor {
        Some(id) => format!("[{text}](#{id})"),
        None => text,
    }
}

/// The images an export copies, each under a file name unique in its `assets/` folder.
#[derive(Default)]
struct Assets {
    names: BTreeMap<VaultPath, String>,
    taken: BTreeSet<String>,
}

impl Assets {
    /// The name of image `path` in `assets/`: its file name, with `-1`, `-2`, … before
    /// the extension when another image has it.
    fn name(&mut self, path: &VaultPath) -> String {
        if let Some(name) = self.names.get(path) {
            return name.clone();
        }
        let file = relative_path::RelativePath::new(path.as_str());
        let stem = file.file_stem().expect("a vault path ends in a name");
        let ext = file.extension().map_or(String::new(), |e| format!(".{e}"));
        let mut name = path.file_name().to_string();
        let mut n = 1;
        while !self.taken.insert(name.clone()) {
            name = format!("{stem}-{n}{ext}");
            n += 1;
        }
        self.names.insert(path.clone(), name.clone());
        name
    }
}

/// Edits that give heading `h` the ATX level `level`, touching only its markers so
/// that links inside the heading text are rewritten separately: everything before the
/// text (an ATX heading's `#` run) becomes the new `#` run; a setext heading's text
/// lines are also joined and its underline dropped.
fn heading_edits(text: &str, h: &Spanned<Heading>, level: u8) -> Vec<Edit> {
    let marks = "#".repeat(level as usize);
    let prefix = if h.content.is_empty() { marks } else { format!("{marks} ") };
    let mut edits = vec![Edit::replace(h.span.start..h.content.span.start, prefix)];
    if h.setext {
        let content = h.content.text(text);
        for (i, _) in content.match_indices('\n') {
            let cr = usize::from(content[..i].ends_with('\r'));
            edits.push(Edit::replace(h.content.span.start + i - cr..h.content.span.start + i + 1, " "));
        }
        let end = h.span.start + text[h.span.clone()].trim_end_matches(['\n', '\r']).len();
        edits.push(Edit::replace(h.content.span.end..end, ""));
    }
    edits
}

fn shift_edits(edits: Vec<Edit>, by: usize) -> Vec<Edit> {
    edits.into_iter().map(|e| Edit { range: e.range.start - by..e.range.end - by, new: e.new }).collect()
}

/// The folder, beside the exported file, that holds the images it embeds.
pub const ASSETS: &str = "assets";

/// Whether an image embed's `|` part is a size (`300` or `300x200`), not alt text.
fn is_size(label: &str) -> bool {
    label.split('x').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::from_texts;

    fn concept(title: &str, body: &str) -> String {
        format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n---\n# {title}\n\n{body}")
    }

    #[test]
    fn anchor_ids() {
        assert_eq!(anchor_id("Dijkstra's algorithm"), "dijkstras-algorithm");
        assert_eq!(anchor_id("1. Intro"), "1-intro");
        assert_eq!(anchor_id("二分查找"), "二分查找");
        assert_eq!(anchor_id("C++ & Rust"), "c--rust");
    }

    #[test]
    fn reading_path_export() {
        let i = from_texts(&[
            ("views/algo.md", "# Algorithms\n\n## Part 1\n\n[[bfs]] and [[dfs|depth-first]].\n"),
            (
                "concepts/a/bfs.md",
                &concept(
                    "BFS",
                    "Uses a queue; see [[queue]].\n\n![[concepts/a/attachments/g.png|graph]]\n\n## Proof\n\nBy induction.\n",
                ),
            ),
            ("concepts/a/dfs.md", &concept("DFS", "Uses a stack.\n")),
            ("concepts/a/queue.md", &concept("Queue", "FIFO.\n")),
            ("concepts/a/attachments/g.png", ""),
        ]);

        let e = export(&i, &VaultPath::new("views/algo.md".into()).unwrap(), 1);
        assert_eq!(e.notes, vec!["views/algo.md", "concepts/a/bfs.md", "concepts/a/dfs.md"]);
        assert_eq!(
            e.assets,
            vec![(VaultPath::new("concepts/a/attachments/g.png".into()).unwrap(), "g.png".to_string())]
        );

        let md = &e.markdown;
        assert!(md.contains("[BFS](#bfs) and [depth-first](#dfs)."), "{md}");
        assert!(md.contains("### BFS\n"), "{md}");
        assert!(md.contains("#### Proof\n"), "{md}");
        assert!(md.contains("see Queue."), "{md}");
        assert!(md.contains("![graph](assets/g.png)"), "{md}");
        assert!(!md.contains("Glossary"), "a note left out is plain text, nothing more: {md}");
        assert!(!md.contains("id:"), "{md}");
    }

    #[test]
    fn section_links_and_depth() {
        let i = from_texts(&[
            ("views/v.md", "# V\n\n[[bfs#Proof]]\n"),
            ("concepts/a/bfs.md", &concept("BFS", "Intro.\n\n## Proof\n\nBy induction.\n\n## Other\n\nX.\n")),
        ]);
        let e = export(&i, &VaultPath::new("views/v.md".into()).unwrap(), 1);
        assert!(e.markdown.contains("[Proof](#proof)"), "{}", e.markdown);
        assert!(e.markdown.contains("By induction."));
        assert!(!e.markdown.contains("Intro."));
        assert!(!e.markdown.contains("X."));
        let none = export(&i, &VaultPath::new("views/v.md".into()).unwrap(), 0);
        assert_eq!(none.notes, vec!["views/v.md"]);
    }

    #[test]
    fn each_note_once_at_first_appearance() {
        let i = from_texts(&[
            ("views/v.md", "# V\n\n[[bfs]], then [[bfs#Proof]].\n"),
            ("concepts/a/bfs.md", &concept("BFS", "Intro.\n\n## Proof\n\nBy induction.\n")),
        ]);
        let e = export(&i, &VaultPath::new("views/v.md".into()).unwrap(), 1);
        assert_eq!(e.notes, vec!["views/v.md", "concepts/a/bfs.md"]);
        assert_eq!(e.markdown.matches("By induction.").count(), 1, "{}", e.markdown);
        assert!(e.markdown.contains("[BFS](#bfs), then [Proof](#proof)."), "{}", e.markdown);
    }

    #[test]
    fn block_links_include_only_the_block() {
        let id = "01JB9XQ7Z3K4M5N6P7Q8R9S0B1";
        let i = from_texts(&[
            ("views/v.md", &format!("# V\n\nSee [[bfs#^{id}]].\n")),
            ("concepts/a/bfs.md", &concept("BFS", &format!("Intro.\n\nThe key step. ^{id}\n\nOther.\n"))),
        ]);
        let e = export(&i, &VaultPath::new("views/v.md".into()).unwrap(), 1);
        let md = &e.markdown;
        assert!(md.contains("The key step.\n") && !md.contains("Intro.") && !md.contains("Other."), "{md}");
        assert!(!md.contains('^'), "block ids are removed: {md}");
        assert!(md.contains("See [BFS](#bfs).") && md.contains("## BFS\n\nThe key step."), "{md}");
    }

    #[test]
    fn headings_with_links_and_setext_headings() {
        let i = from_texts(&[
            ("views/v.md", "# V\n\n## Read [[bfs]] first ##\n\nSetext [[bfs]]\ntwo lines\n---\n\nText.\n"),
            ("concepts/a/bfs.md", &concept("BFS", "Body.\n")),
        ]);
        let md = export(&i, &VaultPath::new("views/v.md".into()).unwrap(), 0).markdown;
        assert!(md.contains("## Read BFS first ##\n"), "{md}");
        assert!(md.contains("## Setext BFS two lines\n\nText."), "{md}");
    }

    #[test]
    fn anchors_are_unique_in_document_order() {
        let i = from_texts(&[
            ("views/v.md", "# BFS notes\n\n## BFS\n\n[[bfs]]\n"),
            ("concepts/a/bfs.md", &concept("BFS", "Body.\n")),
            ("concepts/a/untitled.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TW\n---\nNo heading.\n"),
        ]);
        let e = export(&i, &VaultPath::new("views/v.md".into()).unwrap(), 1);
        assert!(e.markdown.contains("[BFS](#bfs-1)"), "{}", e.markdown);
        let i = from_texts(&[
            ("views/v.md", "# V\n\n[[untitled]]\n"),
            ("concepts/a/untitled.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TW\n---\nNo heading.\n"),
        ]);
        let e = export(&i, &VaultPath::new("views/v.md".into()).unwrap(), 1);
        assert!(
            e.markdown.contains("[untitled](#untitled)") && e.markdown.contains("## untitled\n\nNo heading."),
            "{}",
            e.markdown
        );
    }
}
