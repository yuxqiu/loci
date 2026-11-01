//! `loci export` (m1-m4 §4): assemble one self-contained markdown document from a view
//! or concept and the notes it links to.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Range;

use crate::digest::valid_digest;
use crate::index::{Index, Resolved, heading_key};
use crate::parse::{Heading, Link, LinkForm, Note};
use crate::splice::{self, Edit};

/// The part of a note a link includes (m1-m4 §4 step 1).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Whole,
    /// The section under a heading (`[[x#heading]]`).
    Heading(String),
    /// One block (`[[x#^id]]`).
    Block(String),
}

impl Part {
    fn of(link: &Link) -> Part {
        match link.fragment.as_deref() {
            None => Part::Whole,
            Some(f) => match f.strip_prefix('^') {
                Some(id) => Part::Block(id.to_string()),
                None => Part::Heading(f.to_string()),
            },
        }
    }
}

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

    /// The unit for `part` of `note`, placed under a heading of `level`. `resolve` has
    /// checked that the fragment exists.
    fn child(note: &'a Note, part: &Part, level: u8) -> Self {
        let range = match part {
            Part::Whole => note.body_start()..note.text.len(),
            Part::Heading(f) => section_range(note, f).expect("resolve checked the heading"),
            Part::Block(id) => {
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

/// Pandoc's identifiers for the headings of the units, in document order: the base
/// identifier, with `-1`, `-2`, … appended when it is already taken.
fn anchors(units: &[Unit]) -> Vec<Anchors> {
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut unique = |text: &str| {
        let base = pandoc_id(text);
        let id = (0..).map(|n| if n == 0 { base.clone() } else { format!("{base}-{n}") }).find(|c| !used.contains(c));
        let id = id.expect("an unused identifier exists");
        used.insert(id.clone());
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
    out
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Export {
    pub markdown: String,
    /// Included note paths, in order.
    pub notes: Vec<String>,
    /// Notes listed in the glossary.
    pub glossary: Vec<String>,
    /// Images to copy: (vault path, file name under `assets/`).
    pub assets: Vec<(String, String)>,
}

/// Pandoc's automatic identifier for a heading text.
fn pandoc_id(text: &str) -> String {
    let mut s: String = text
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
                Some(c.to_lowercase().collect::<String>())
            } else if c.is_whitespace() {
                Some("-".to_string())
            } else {
                None
            }
        })
        .collect();
    // Remove everything up to the first letter.
    let first_letter = s.char_indices().find(|(_, c)| c.is_alphabetic()).map_or(s.len(), |(i, _)| i);
    s.drain(..first_letter);
    if s.is_empty() { "section".to_string() } else { s }
}

fn is_image(path: &str) -> bool {
    let lower = path.to_lowercase();
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

pub fn export(index: &Index, root: &str, depth: usize) -> Export {
    // 1. Collect: each note once, at its first appearance, breadth-first up to `depth`.
    let mut units = vec![Unit::root(index.parsed(root).expect("the command resolved `root` to a readable note"))];
    let mut unit_of: BTreeMap<&str, usize> = BTreeMap::from([(root, 0)]);
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
            let path = note.path.as_str();
            if unit_of.contains_key(path) || children.iter().any(|(q, _): &(&str, Unit)| *q == path) {
                continue;
            }
            let level = (parent.level_at(link.range.start) + 1).min(6);
            children.push((path, Unit::child(note, &Part::of(link), level)));
        }
        for (path, unit) in children {
            unit_of.insert(path, units.len());
            queue.push_back((units.len(), d + 1));
            units.push(unit);
        }
    }
    let anchors = anchors(&units);

    // 2–4. Assemble, rewriting headings, links, images and block ids.
    let mut out = Export::default();
    let mut asset_names: BTreeMap<String, String> = BTreeMap::new();
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
            let label = link_label(note, link);
            let replacement = match resolved {
                Resolved::File(p) if link.form == LinkForm::Embed && is_image(&p) => {
                    let name = asset_names.entry(p.clone()).or_insert_with(|| unique_name(&p, &mut taken)).clone();
                    let alt = embed_alt(note, link).unwrap_or_else(|| name.clone());
                    format!("![{alt}](assets/{name})")
                }
                Resolved::File(_) => label.unwrap_or_else(|| link.target.clone()),
                Resolved::Note(p) => {
                    let target = index.parsed(&p).expect("a resolved note");
                    let heading = link.fragment.as_deref().filter(|f| !f.starts_with('^'));
                    let text = label.unwrap_or_else(|| heading.unwrap_or(target.title()).to_string());
                    match unit_of.get(p.as_str()) {
                        Some(&j) => {
                            let a = &anchors[j];
                            match heading.and_then(|f| a.headings.get(&heading_key(f))).or(a.top.as_ref()) {
                                Some(id) => format!("[{text}](#{id})"),
                                None => text,
                            }
                        }
                        None => {
                            if valid_digest(target).is_some() && !out.glossary.contains(&p) {
                                out.glossary.push(p.clone());
                            }
                            text
                        }
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
    if !out.glossary.is_empty() {
        doc.push_str(if doc.ends_with('\n') { "\n## Glossary\n\n" } else { "\n\n## Glossary\n\n" });
        for p in &out.glossary {
            let n = index.parsed(p).expect("the glossary holds notes");
            let digest = valid_digest(n).expect("only notes with a valid digest are in the glossary");
            doc.push_str(&format!("**{}**: {digest}\n\n", n.title()));
        }
    }
    out.markdown = format!("{}\n", doc.trim_end());
    out.assets = asset_names.into_iter().collect();
    out
}

/// Edits that give heading `h` the ATX level `level`, touching only its markers so
/// that links inside the heading text are rewritten separately: an ATX heading's `#`
/// run is replaced; a setext heading gets a `#` prefix, its text lines are joined and
/// its underline is dropped.
fn heading_edits(text: &str, h: &Heading, level: u8) -> Vec<Edit> {
    let marks = "#".repeat(level as usize);
    let start = h.range.start;
    let raw = text[h.range.clone()].trim_end_matches(['\n', '\r']);
    let indent = raw.len() - raw.trim_start_matches(' ').len();
    if raw[indent..].starts_with('#') {
        let run = raw[indent..].len() - raw[indent..].trim_start_matches('#').len();
        return vec![Edit::replace(start..start + indent + run, marks)];
    }
    let underline = raw.rfind('\n').expect("a setext heading ends with an underline line");
    let mut edits = vec![Edit::insert(start, format!("{marks} "))];
    let content = raw[..underline].trim_end_matches('\r');
    for (i, _) in content.match_indices('\n') {
        let cr = usize::from(content[..i].ends_with('\r'));
        edits.push(Edit::replace(start + i - cr..start + i + 1, " "));
    }
    edits.push(Edit::replace(start + content.len()..start + raw.len(), ""));
    edits
}

fn shift_edits(edits: Vec<Edit>, by: usize) -> Vec<Edit> {
    edits.into_iter().map(|e| Edit { range: e.range.start - by..e.range.end - by, new: e.new }).collect()
}

/// The `|label` of a wikilink, if any.
fn link_label(note: &Note, link: &Link) -> Option<String> {
    let after = &note.text[link.target_range.end..link.range.end];
    let label = after.strip_prefix('|')?.strip_suffix("]]")?;
    Some(label.to_string())
}

/// The alt text of an image embed: the `|` part when it is not a size.
fn embed_alt(note: &Note, link: &Link) -> Option<String> {
    let l = link_label(note, link)?;
    let is_size = l.split('x').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    (!is_size).then_some(l)
}

fn unique_name(path: &str, taken: &mut BTreeSet<String>) -> String {
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    let (stem, ext) = match name.rfind('.') {
        Some(i) => (name[..i].to_string(), name[i..].to_string()),
        None => (name.clone(), String::new()),
    };
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
        format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\ndigest: About {title}.\n---\n# {title}\n\n{body}")
    }

    #[test]
    fn pandoc_ids() {
        assert_eq!(pandoc_id("Dijkstra's algorithm"), "dijkstras-algorithm");
        assert_eq!(pandoc_id("1. Intro"), "intro");
        assert_eq!(pandoc_id("二分查找"), "二分查找");
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
        let e = export(&i, "views/algo.md", 1);
        assert_eq!(e.notes, vec!["views/algo.md", "concepts/a/bfs.md", "concepts/a/dfs.md"]);
        assert_eq!(e.glossary, vec!["concepts/a/queue.md"]);
        assert_eq!(e.assets, vec![("concepts/a/attachments/g.png".to_string(), "g.png".to_string())]);
        let md = &e.markdown;
        assert!(md.contains("[BFS](#bfs) and [depth-first](#dfs)."), "{md}");
        assert!(md.contains("### BFS\n"), "{md}");
        assert!(md.contains("#### Proof\n"), "{md}");
        assert!(md.contains("see Queue."), "{md}");
        assert!(md.contains("![graph](assets/g.png)"), "{md}");
        assert!(md.contains("## Glossary\n\n**Queue**: About Queue."), "{md}");
        assert!(!md.contains("digest:"), "{md}");
    }

    #[test]
    fn section_links_and_depth() {
        let i = from_texts(&[
            ("views/v.md", "# V\n\n[[bfs#Proof]]\n"),
            ("concepts/a/bfs.md", &concept("BFS", "Intro.\n\n## Proof\n\nBy induction.\n\n## Other\n\nX.\n")),
        ]);
        let e = export(&i, "views/v.md", 1);
        assert!(e.markdown.contains("[Proof](#proof)"), "{}", e.markdown);
        assert!(e.markdown.contains("By induction."));
        assert!(!e.markdown.contains("Intro."));
        assert!(!e.markdown.contains("X."));
        let none = export(&i, "views/v.md", 0);
        assert_eq!(none.notes, vec!["views/v.md"]);
    }

    #[test]
    fn each_note_once_at_first_appearance() {
        let i = from_texts(&[
            ("views/v.md", "# V\n\n[[bfs]], then [[bfs#Proof]].\n"),
            ("concepts/a/bfs.md", &concept("BFS", "Intro.\n\n## Proof\n\nBy induction.\n")),
        ]);
        let e = export(&i, "views/v.md", 1);
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
        let e = export(&i, "views/v.md", 1);
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
        let md = export(&i, "views/v.md", 0).markdown;
        assert!(md.contains("## Read BFS first ##\n"), "{md}");
        assert!(md.contains("## Setext BFS two lines\n\nText."), "{md}");
    }

    #[test]
    fn anchors_follow_pandoc_uniqueness() {
        let i = from_texts(&[
            ("views/v.md", "# BFS notes\n\n## BFS\n\n[[bfs]]\n"),
            ("concepts/a/bfs.md", &concept("BFS", "Body.\n")),
            ("concepts/a/untitled.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TW\ndigest: d\n---\nNo heading.\n"),
        ]);
        let e = export(&i, "views/v.md", 1);
        assert!(e.markdown.contains("[BFS](#bfs-1)"), "{}", e.markdown);
        let i = from_texts(&[
            ("views/v.md", "# V\n\n[[untitled]]\n"),
            ("concepts/a/untitled.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TW\ndigest: d\n---\nNo heading.\n"),
        ]);
        let e = export(&i, "views/v.md", 1);
        assert!(
            e.markdown.contains("[untitled](#untitled)") && e.markdown.contains("## untitled\n\nNo heading."),
            "{}",
            e.markdown
        );
    }
}
