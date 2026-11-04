//! `loci export` (m1-m4 §4): assemble one self-contained markdown document from a view
//! or concept and the notes it links to.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Range;

use crate::index::{Index, Resolved};
use crate::parse::{self, Fragment, Heading, Link, LinkForm, Note, Spanned, heading_key};
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
    /// a heading, or one block, as `resolve` found it), placed under a heading of `level`.
    fn child(note: &'a Note, part: Option<Range<usize>>, level: u8) -> Self {
        let range = part.unwrap_or(note.body_start()..note.text.len());
        // Its top heading takes `level`; without one, a title does, above its headings.
        let mut unit = Unit { note, range, shift: 0, title: None };
        let headings = unit.headings();
        match headings.first() {
            Some(h) if h.span.start == unit.content_start() => unit.shift = level as i32 - h.level as i32,
            _ => {
                let min = headings.iter().map(|h| h.level).min();
                unit.shift = min.map_or(0, |m| level as i32 + 1 - m as i32);
                unit.title = Some((level, note.title().to_string()));
            }
        }
        unit
    }

    /// The headings of the unit's bytes; they borrow the note, not the unit.
    fn headings(&self) -> Vec<Spanned<&'a Heading>> {
        let range = &self.range;
        self.note.headings().filter(|h| range.start <= h.span.start && h.span.end <= range.end).collect()
    }

    /// The first non-whitespace byte of the unit.
    fn content_start(&self) -> usize {
        let text = &self.note.text[self.range.clone()];
        self.range.start + (text.len() - text.trim_start().len())
    }

    /// The unit's links to notes and files, in order, each with what it resolves to.
    fn links(&self, index: &'a Index) -> Vec<(&'a Spanned<Link>, Option<Resolved<'a>>)> {
        let note = self.note;
        note.links
            .iter()
            .filter(|l| matches!(l.form, LinkForm::Wiki | LinkForm::Embed))
            .filter(|l| self.range.start <= l.span.start && l.span.end <= self.range.end)
            .map(|l| (l, index.resolve(note, l).ok()))
            .collect()
    }

    fn level(&self, h: &Spanned<&Heading>) -> u8 {
        (h.level as i32 + self.shift).clamp(1, 6) as u8
    }

    /// The output level of the nearest heading before byte `at` (0 if none).
    fn level_at(&self, at: usize) -> u8 {
        match self.headings().iter().rfind(|h| h.span.start < at) {
            Some(h) => self.level(h),
            None => self.title.as_ref().map_or(0, |(l, _)| *l),
        }
    }
}

/// Anchors of one unit in the assembled document.
#[derive(Default)]
struct Anchors {
    /// The unit's top heading (its inserted title, or the heading it starts with).
    top: Option<String>,
    /// By heading key (§5.4), the first heading with that key.
    headings: BTreeMap<String, String>,
}

/// The heading identifiers of a document, each [`anchor_id`] of the heading text with
/// `-1`, `-2`, … appended when it is already taken, as GitHub does.
#[derive(Default)]
struct AnchorIds {
    used: BTreeSet<String>,
    /// Every identifier, in document order.
    order: Vec<String>,
}

impl AnchorIds {
    /// The identifier of the next heading, whose text is `text`.
    fn take(&mut self, text: &str) -> String {
        let base = anchor_id(text);
        let id = take_unique(&mut self.used, |n| if n == 0 { base.clone() } else { format!("{base}-{n}") });
        self.order.push(id.clone());
        id
    }
}

/// The first of `candidate(0)`, `candidate(1)`, … not in `taken`, now taken.
fn take_unique(taken: &mut BTreeSet<String>, candidate: impl Fn(usize) -> String) -> String {
    let name = (0..).map(candidate).find(|c| !taken.contains(c)).expect("an untaken name exists");
    taken.insert(name.clone());
    name
}

/// The identifiers of the headings of the units: per unit, its anchors; and every
/// identifier in document order.
fn anchors(units: &[Unit]) -> (Vec<Anchors>, Vec<String>) {
    let mut ids = AnchorIds::default();
    let mut out = vec![];
    for u in units {
        let mut a = Anchors::default();
        if let Some((_, title)) = &u.title {
            a.top = Some(ids.take(title));
        }
        let start = u.content_start();
        for h in u.headings() {
            let text = parse::one_line(h.content.value(&u.note.text));
            let id = ids.take(&text);
            if a.top.is_none() && h.span.start == start {
                a.top = Some(id.clone());
            }
            a.headings.entry(heading_key(&text)).or_insert(id);
        }
        out.push(a);
    }
    (out, ids.order)
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

impl Export {
    pub fn of(index: &Index, root: &Note, depth: usize) -> Export {
        // 1. Collect, then name every heading once.
        let (units, unit_of) = collect(index, root, depth);
        let (anchors, heading_ids) = anchors(&units);

        // 2–4. Assemble, rewriting headings, links, images and block ids.
        let mut assembly = Assembly { index, unit_of, anchors, assets: Assets::default() };
        let mut doc = String::new();
        for u in &units {
            let edited = assembly.unit_text(u);
            append(&mut doc, u.title.as_ref(), &edited);
        }

        let notes = units.iter().map(|u| u.note.path.clone()).collect();
        let assets = assembly.assets.names.into_iter().collect();
        Export { markdown: format!("{}\n", doc.trim_end()), notes, assets, heading_ids }
    }
}

/// Appends a unit's `edited` text to `doc`, a blank line apart, under its inserted
/// `title` (level, text) if it has one.
fn append(doc: &mut String, title: Option<&(u8, String)>, edited: &str) {
    if !doc.is_empty() && !doc.ends_with("\n\n") {
        doc.push_str(if doc.ends_with('\n') { "\n" } else { "\n\n" });
    }
    if let Some((level, title)) = title {
        doc.push_str(&format!("{} {title}\n\n", "#".repeat(*level as usize)));
    }
    doc.push_str(edited.trim_start_matches('\n'));
}

/// What rewriting a unit needs: the index, where each included note is, the units'
/// anchors, and the images collected so far.
struct Assembly<'a> {
    index: &'a Index<'a>,
    unit_of: BTreeMap<&'a VaultPath, usize>,
    anchors: Vec<Anchors>,
    assets: Assets,
}

impl Assembly<'_> {
    /// The text of unit `u` with its headings set to their levels, its block ids dropped
    /// and its links rewritten.
    fn unit_text(&mut self, u: &Unit) -> String {
        let note = u.note;
        let inside = |span: &Range<usize>| u.range.start <= span.start && span.end <= u.range.end;
        let mut edits: Vec<Edit> = u.headings().iter().flat_map(|h| heading_edits(&note.text, h, u.level(h))).collect();

        // Block ids are vault plumbing: dropped, with the whitespace before them (an id
        // ends its block, so no line is joined to the next). Where the
        // line left would change the shape of the note's blocks (`===` alone is a heading underline),
        // an empty HTML comment keeps it text, and renders as nothing.
        for id in note.block_ids().map(|b| b.id).filter(|id| inside(&id.span)) {
            let start = note.text[..id.span.start].trim_end_matches([' ', '\t', '\r', '\n']).len();
            let dropped = Edit::replace(start..id.span.end, "");
            let left = splice::apply(&note.text, vec![dropped.clone()]).expect("one edit inside the text");
            if parse::markdown(left).same_shape(note) {
                edits.push(dropped);
            } else {
                edits.push(Edit::replace(id.span.clone(), "<!-- -->"));
            }
        }

        for (link, resolved) in u.links(self.index) {
            let new = self.link_replacement(note, link, resolved);
            edits.push(Edit::replace(link.span.clone(), new));
        }

        // The text around the unit goes too; the edits above lie inside it.
        edits.push(Edit::replace(0..u.range.start, ""));
        edits.push(Edit::replace(u.range.end..note.text.len(), ""));
        splice::apply(&note.text, edits).expect("heading, block id and link edits never overlap")
    }

    /// What `link` in `note` becomes in the document: an image under `assets/`, a link
    /// to an included note's anchor, or plain text (its label, heading or title; for a
    /// link that does not resolve, its target as written) for anything left out.
    fn link_replacement(&mut self, note: &Note, link: &Spanned<Link>, resolved: Option<Resolved<'_>>) -> String {
        let label = link.label.as_ref().map(|l| l.value(&note.text).to_string());
        let linked = match resolved {
            None => return label.unwrap_or_else(|| link.target.text(&note.text).to_string()),
            Some(Resolved::File(p)) if link.form == LinkForm::Embed && is_image(p) => {
                let name = self.assets.name(p);
                let alt = label.filter(|l| !is_size(l)).unwrap_or_else(|| name.clone());
                return format!("![{alt}]({ASSETS}/{name})");
            }
            Some(Resolved::File(p)) => return label.unwrap_or_else(|| p.to_string()),
            Some(Resolved::Note(n, _)) => n,
        };

        let heading = match &link.target.fragment {
            Some(Fragment::Heading(h)) => Some(h.as_str()),
            Some(Fragment::Block(_)) | None => None,
        };

        let text = label.unwrap_or_else(|| heading.unwrap_or_else(|| linked.title()).to_string());
        let anchor = self.unit_of.get(&linked.path).and_then(|&j| {
            let a = &self.anchors[j];
            heading.and_then(|h| a.headings.get(&heading_key(h))).or(a.top.as_ref())
        });

        match anchor {
            Some(id) => format!("[{text}](#{id})"),
            None => text,
        }
    }
}

/// Step 1 (m1-m4 §4): each note once, at its first appearance, breadth-first from
/// `root` up to `depth` links away; and each note's unit index.
fn collect<'a>(index: &'a Index, root: &'a Note, depth: usize) -> (Vec<Unit<'a>>, BTreeMap<&'a VaultPath, usize>) {
    let mut units = vec![Unit::root(root)];
    let mut unit_of: BTreeMap<&VaultPath, usize> = BTreeMap::from([(&root.path, 0)]);
    let mut queue = VecDeque::from([(0usize, 0usize)]);
    while let Some((i, d)) = queue.pop_front() {
        if d >= depth {
            continue;
        }

        // The notes the unit links to, each placed one level below the heading before
        // its link; read before any is added, since adding moves `units`.
        let parent = &units[i];
        let children: Vec<(&Note, Option<Range<usize>>, u8)> = parent
            .links(index)
            .into_iter()
            .filter_map(|(link, resolved)| match resolved {
                Some(Resolved::Note(note, part)) => Some((link, note, part)),
                Some(Resolved::File(_)) | None => None,
            })
            .map(|(link, note, part)| (note, part, (parent.level_at(link.span.start) + 1).min(6)))
            .collect();

        for (note, part, level) in children {
            if unit_of.contains_key(&note.path) {
                continue;
            }
            unit_of.insert(&note.path, units.len());
            queue.push_back((units.len(), d + 1));
            units.push(Unit::child(note, part, level));
        }
    }

    (units, unit_of)
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
        let name = take_unique(&mut self.taken, |n| {
            if n == 0 { path.file_name().to_string() } else { format!("{stem}-{n}{ext}") }
        });
        self.names.insert(path.clone(), name.clone());
        name
    }
}

/// Edits that give heading `h` the ATX level `level`, touching only its markers so
/// that links inside the heading text are rewritten separately: everything before the
/// text (an ATX heading's `#` run) becomes the new `#` run; a setext heading's text
/// lines are also joined on one line ([`parse::line_ends`] as spaces) and its underline
/// dropped.
fn heading_edits(text: &str, h: &Spanned<&Heading>, level: u8) -> Vec<Edit> {
    let marks = "#".repeat(level as usize);
    let prefix = if h.content.span.is_empty() { marks } else { format!("{marks} ") };
    let mut edits = vec![Edit::replace(h.span.start..h.content.span.start, prefix)];
    if h.setext {
        let start = h.content.span.start;
        let ends = parse::line_ends(h.content.value(text));
        edits.extend(ends.into_iter().map(|e| Edit::replace(start + e.start..start + e.end, " ")));
        let end = h.span.start + text[h.span.clone()].trim_end_matches(['\n', '\r']).len();
        edits.push(Edit::replace(h.content.span.end..end, ""));
    }
    edits
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
    use crate::index::Index;
    use crate::testing::vp;

    fn concept(title: &str, body: &str) -> String {
        format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n---\n# {title}\n\n{body}")
    }

    fn export(i: &Index, root: &str, depth: usize) -> Export {
        Export::of(i, i.note_at(&vp(root)).unwrap(), depth)
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
        let i = crate::testing::index(&[
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
        assert_eq!(e.assets, vec![(vp("concepts/a/attachments/g.png"), "g.png".to_string())]);

        let md = &e.markdown;
        assert!(md.contains("[BFS](#bfs) and [depth-first](#dfs)."), "{md}");
        assert!(md.contains("### BFS\n"), "{md}");
        assert!(md.contains("#### Proof\n"), "{md}");
        assert!(md.contains("see Queue."), "{md}");
        assert!(md.contains("![graph](assets/g.png)"), "{md}");
        assert!(!md.contains("id:"), "{md}");
    }

    #[test]
    fn section_links_and_depth() {
        let i = crate::testing::index(&[
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
    fn unresolved_links_become_plain_text() {
        let i = crate::testing::index(&[("views/v.md", "# V\n\nSee [[nope]], [[nope|that]] and [[#Missing]].\n")]);
        assert_eq!(export(&i, "views/v.md", 1).markdown, "# V\n\nSee nope, that and #Missing.\n");
    }

    #[test]
    fn each_note_once_at_first_appearance() {
        let i = crate::testing::index(&[
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
        let i = crate::testing::index(&[
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
        let i = crate::testing::index(&[
            ("views/v.md", "# V\n\n## Read [[bfs]] first ##\n\nSetext [[bfs]]\ntwo lines\n---\n\nText.\n"),
            ("concepts/a/bfs.md", &concept("BFS", "Body.\n")),
        ]);
        let md = export(&i, "views/v.md", 0).markdown;
        assert!(md.contains("## Read BFS first ##\n"), "{md}");
        assert!(md.contains("## Setext BFS two lines\n\nText."), "{md}");
    }

    #[test]
    fn anchors_are_unique_in_document_order() {
        let i = crate::testing::index(&[
            ("views/v.md", "# BFS notes\n\n## BFS\n\n[[bfs]]\n"),
            ("concepts/a/bfs.md", &concept("BFS", "Body.\n")),
            ("concepts/a/untitled.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TW\n---\nNo heading.\n"),
        ]);
        let e = export(&i, "views/v.md", 1);
        assert!(e.markdown.contains("[BFS](#bfs-1)"), "{}", e.markdown);
        let i = crate::testing::index(&[
            ("views/v.md", "# V\n\n[[untitled]]\n"),
            ("concepts/a/untitled.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TW\n---\nNo heading.\n"),
        ]);
        let e = export(&i, "views/v.md", 1);
        assert!(
            e.markdown.contains("[untitled](#untitled)") && e.markdown.contains("## untitled\n\nNo heading."),
            "{}",
            e.markdown
        );
    }

    /// A paragraph's last line that is only `===` or `***` once its block id goes stays
    /// paragraph text, not a heading underline or a rule.
    #[test]
    fn a_dropped_block_id_leaves_no_new_block() {
        for (line, kept) in [("=== ^b1", "=== <!-- -->"), ("--- ^b1", "--- <!-- -->"), ("*** ^b1", "*** <!-- -->")] {
            let i = crate::testing::index(&[("concepts/a/x.md", concept("X", &format!("Some text\n{line}\n")))]);
            let md = export(&i, "concepts/a/x.md", 0).markdown;
            assert!(md.contains(&format!("Some text\n{kept}\n")), "{line}: {md}");
            assert_eq!(crate::parse::markdown(md.clone()).headings().count(), 1, "{md}");
        }
        for (body, kept) in [
            ("> Some text\n> === ^b1\n", "> === <!-- -->\n"),
            ("*** ^b1\n", "\n*** <!-- -->\n"),
            ("Para\n- ^b2\n", "Para\n- <!-- -->\n"),
            ("Para ^b3\n", "\nPara\n"),
            ("- a\n  - ^b4\n", "- a\n  - <!-- -->\n"),
            ("- a\n- -- ^b5\n- c\n", "- a\n- -- <!-- -->\n- c\n"),
            // The whitespace before a kept comment stays: it sets the item's content column.
            ("-  ^b6\n  -\n", "-  <!-- -->\n  -\n"),
            // An id on a line of its own goes with the line, so the list stays tight.
            ("- a\n  ^b7\n- c\n", "- a\n- c\n"),
        ] {
            let i = crate::testing::index(&[("views/v.md", format!("# V\n\n{body}"))]);
            let md = export(&i, "views/v.md", 0).markdown;
            assert!(md.contains(kept), "{body}: {md}");
        }
        // A list item holding only its id keeps its marker.
        let i = crate::testing::index(&[("views/v.md", "# V\n\n- a\n- ^b1\n")]);
        assert!(export(&i, "views/v.md", 0).markdown.contains("- a\n-\n"));
    }

    /// A setext heading's lines are joined with one space, the spaces and tabs around each
    /// line ending going too, inside a code span as well (a hard break's backslash stays).
    /// A heading inside a block quote or an item is no heading of the note (§4.6): left as
    /// written.
    #[test]
    fn a_setext_heading_becomes_one_atx_line() {
        for (body, want) in [
            ("Hard  \r\n   break\\\r\nhere\r\n===\r\n", "# Hard break\\ here\n"),
            ("a `b\nc` d\n===\n", "# a `b c` d\n"),
            ("Title \nmore\n---\n", "## Title more\n"),
            ("> First line\n> second line\n> ===\n", "> First line\n> second line\n> ===\n"),
            ("- Item line\n  more\n  ---\n", "- Item line\n  more\n  ---\n"),
        ] {
            let i = crate::testing::index(&[("views/v.md", format!("# V\n\n{body}"))]);
            let md = export(&i, "views/v.md", 0).markdown;
            assert!(md.contains(want), "{body:?}: {md:?}");
        }
    }
}
