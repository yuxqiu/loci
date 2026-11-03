//! `loci export` (m1-m4 §4): assemble one self-contained markdown document from a view
//! or concept and the notes it links to.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Range;

use crate::index::{Index, Resolved, heading_key};
use crate::parse::{Fragment, Heading, Link, LinkForm, Note};
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
                note.block_ids.iter().find(|b| &b.id == id).expect("resolve checked the block id").block.clone()
            }
        };
        let headings: Vec<&Heading> = headings_in(note, &range).collect();
        match headings.first() {
            Some(h) if h.range.start == content_start(note, &range) => {
                Unit { note, range, shift: level as i32 - h.level as i32, title: None }
            }
            _ => {
                let min = headings.iter().map(|h| h.level).min();
                let shift = min.map_or(0, |m| level as i32 + 1 - m as i32);
                Unit { note, range, shift, title: Some((level, note.title().to_string())) }
            }
        }
    }

    fn level(&self, h: &Heading) -> u8 {
        (h.level as i32 + self.shift).clamp(1, 6) as u8
    }

    /// The output level of the nearest heading before byte `at` (0 if none).
    fn level_at(&self, at: usize) -> u8 {
        match headings_in(self.note, &self.range).rfind(|h| h.range.start < at) {
            Some(h) => self.level(h),
            None => self.title.as_ref().map_or(0, |(l, _)| *l),
        }
    }
}

/// The headings of `note` inside `range`.
fn headings_in<'n>(note: &'n Note, range: &Range<usize>) -> impl DoubleEndedIterator<Item = &'n Heading> {
    let range = range.clone();
    note.headings.iter().filter(move |h| range.start <= h.range.start && h.range.end <= range.end)
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
            let id = unique(&h.text);
            if a.top.is_none() && h.range.start == start {
                a.top = Some(id.clone());
            }
            a.headings.entry(heading_key(&h.text)).or_insert(id);
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
    let i = note.headings.iter().position(|h| heading_key(&h.text) == key)?;
    Some(note.section(i))
}

/// The note links of `note` within `range`, resolved, in order.
fn note_links<'a>(index: &Index, note: &'a Note, range: &Range<usize>) -> Vec<(&'a Link, Resolved)> {
    note.links
        .iter()
        .filter(|l| matches!(l.form, LinkForm::Wiki | LinkForm::Embed))
        .filter(|l| range.start <= l.range.start && l.range.end <= range.end)
        .filter_map(|l| index.resolve(note, l).ok().map(|r| (l, r)))
        .collect()
}

pub fn export(index: &Index, root: &VaultPath, depth: usize) -> Export {
    // 1. Collect: each note once, at its first appearance, breadth-first up to `depth`.
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
            let level = (parent.level_at(link.range.start) + 1).min(6);
            children.push((path, Unit::child(note, link.fragment.as_ref(), level)));
        }
        for (path, unit) in children {
            unit_of.insert(path, units.len());
            queue.push_back((units.len(), d + 1));
            units.push(unit);
        }
    }

    let (anchors, heading_ids) = anchors(&units);

    // 2–4. Assemble, rewriting headings, links, images and block ids.
    let mut out = Export { heading_ids, ..Export::default() };
    let mut asset_names: BTreeMap<VaultPath, String> = BTreeMap::new();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut doc = String::new();
    for u in &units {
        let note = u.note;
        let mut edits: Vec<Edit> = vec![];
        for h in headings_in(note, &u.range) {
            edits.extend(heading_edits(&note.text, h, u.level(h)));
        }
        for b in note.block_ids.iter().filter(|b| u.range.start <= b.range.start && b.range.end <= u.range.end) {
            let before = &note.text[..b.range.start];
            let start = before.trim_end_matches([' ', '\t']).len();
            edits.push(Edit::replace(start..b.range.end, ""));
        }
        for (link, resolved) in note_links(index, note, &u.range) {
            let label = link.label.clone().map(|r| note.text[r].to_string());
            let replacement = match resolved {
                Resolved::File(p) if link.form == LinkForm::Embed && is_image(&p) => {
                    let name = asset_names.entry(p.clone()).or_insert_with(|| unique_name(&p, &mut taken)).clone();
                    let alt = label.filter(|l| !is_size(l)).unwrap_or_else(|| name.clone());
                    format!("![{alt}]({ASSETS}/{name})")
                }
                Resolved::File(_) => label.unwrap_or_else(|| link.target.clone()),
                Resolved::Note(p) => {
                    let target = index.parsed(&p).expect("a resolved note");
                    let heading = match &link.fragment {
                        Some(Fragment::Heading(h)) => Some(h.as_str()),
                        Some(Fragment::Block(_)) | None => None,
                    };
                    let text = label.unwrap_or_else(|| heading.unwrap_or(target.title()).to_string());
                    match unit_of.get(&p) {
                        Some(&j) => {
                            let a = &anchors[j];
                            match heading.and_then(|f| a.headings.get(&heading_key(f))).or(a.top.as_ref()) {
                                Some(id) => format!("[{text}](#{id})"),
                                None => text,
                            }
                        }
                        None => text,
                    }
                }
            };
            edits.push(Edit::replace(link.range.clone(), replacement));
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
        out.notes.push(note.path.clone());
    }

    out.markdown = format!("{}\n", doc.trim_end());
    out.assets = asset_names.into_iter().collect();
    out
}

/// Edits that give heading `h` the ATX level `level`, touching only its markers so
/// that links inside the heading text are rewritten separately: everything before the
/// text (an ATX heading's `#` run) becomes the new `#` run; a setext heading's text
/// lines are also joined and its underline dropped.
fn heading_edits(text: &str, h: &Heading, level: u8) -> Vec<Edit> {
    let marks = "#".repeat(level as usize);
    let prefix = if h.content.is_empty() { marks } else { format!("{marks} ") };
    let mut edits = vec![Edit::replace(h.range.start..h.content.start, prefix)];
    if h.setext {
        let content = &text[h.content.clone()];
        for (i, _) in content.match_indices('\n') {
            let cr = usize::from(content[..i].ends_with('\r'));
            edits.push(Edit::replace(h.content.start + i - cr..h.content.start + i + 1, " "));
        }
        let end = h.range.start + text[h.range.clone()].trim_end_matches(['\n', '\r']).len();
        edits.push(Edit::replace(h.content.end..end, ""));
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

fn unique_name(path: &VaultPath, taken: &mut BTreeSet<String>) -> String {
    let name = path.file_name().to_string();
    let file = relative_path::RelativePath::new(path.as_str());
    let stem = file.file_stem().expect("a vault path ends in a name");
    let ext = file.extension().map_or(String::new(), |e| format!(".{e}"));
    let mut candidate = name.clone();
    let mut n = 1;
    while !taken.insert(candidate.clone()) {
        candidate = format!("{stem}-{n}{ext}");
        n += 1;
    }
    candidate
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
