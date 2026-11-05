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
    /// A heading inserted before the content, when the content does not start with a
    /// heading.
    title: Option<Title>,
}

/// A heading export inserts: the included note's title, at the unit's level.
struct Title {
    level: u8,
    text: String,
}

impl<'a> Unit<'a> {
    fn root(note: &'a Note) -> Self {
        Unit { note, range: note.body_start()..note.text.len(), shift: 0, title: None }
    }

    /// The unit for `part` of `note` (m1-m4 §4 step 1: the whole note, the section under
    /// a heading, or one block, as `resolve` found it), its top heading at `level`.
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
                unit.title = Some(Title { level, text: note.title().into_owned() });
            }
        }
        unit
    }

    /// The headings of the unit's bytes; they borrow the note, not the unit.
    fn headings(&self) -> Vec<Spanned<&'a Heading>> {
        self.note.headings().filter(|h| self.contains(&h.span)).collect()
    }

    /// Whether `span` of the note lies in the unit.
    fn contains(&self, span: &Range<usize>) -> bool {
        self.range.start <= span.start && span.end <= self.range.end
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
            .filter(|l| self.contains(&l.span))
            .map(|l| (l, index.resolve(note, l).ok()))
            .collect()
    }

    fn level(&self, h: &Spanned<&Heading>) -> u8 {
        (h.level as i32 + self.shift).clamp(1, 6) as u8
    }

    /// The output level of the nearest heading before byte `at`; else the inserted
    /// title's; 0 when there is neither.
    fn level_at(&self, at: usize) -> u8 {
        match self.headings().iter().rfind(|h| h.span.start < at) {
            Some(h) => self.level(h),
            None => self.title.as_ref().map_or(0, |t| t.level),
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

/// The first of `candidate(0)`, `candidate(1)`, … not in `taken`, now taken.
fn take_unique(taken: &mut BTreeSet<String>, candidate: impl Fn(usize) -> String) -> String {
    let name = (0..).map(candidate).find(|c| !taken.contains(c)).expect("an untaken name exists");
    taken.insert(name.clone());
    name
}

/// The identifiers of the headings of the units: by note path, its unit's anchors; and every
/// identifier in document order. Each is the [`anchor_id`] of the heading text with
/// `-1`, `-2`, … appended when it is already taken, as GitHub does.
fn anchors<'a>(units: &[Unit<'a>]) -> (BTreeMap<&'a VaultPath, Anchors>, Vec<String>) {
    let (mut used, mut order) = (BTreeSet::new(), vec![]);
    let mut take = |text: &str| {
        let base = anchor_id(text);
        let id = take_unique(&mut used, |n| if n == 0 { base.clone() } else { format!("{base}-{n}") });
        order.push(id.clone());
        id
    };
    let mut out = BTreeMap::new();
    for u in units {
        let mut a = Anchors::default();
        if let Some(title) = &u.title {
            a.top = Some(take(&title.text));
        }
        let start = u.content_start();
        for h in u.headings() {
            let text = h.heading_text(&u.note.text);
            let id = take(&text);
            if a.top.is_none() && h.span.start == start {
                a.top = Some(id.clone());
            }
            a.headings.entry(heading_key(&text)).or_insert(id);
        }
        out.insert(&u.note.path, a);
    }
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
/// kept; each space a `-`; everything else dropped; `section` if nothing is left.
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
        let units = collect(index, root, depth);
        let (anchors, heading_ids) = anchors(&units);

        // 2–4. Assemble, rewriting headings, links, images and block ids.
        let mut assembly = Assembly { index, anchors, assets: Assets::default() };
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
/// `title` if it has one.
fn append(doc: &mut String, title: Option<&Title>, edited: &str) {
    if !doc.is_empty() && !doc.ends_with("\n\n") {
        doc.push_str(if doc.ends_with('\n') { "\n" } else { "\n\n" });
    }
    if let Some(Title { level, text }) = title {
        doc.push_str(&format!("{} {text}{}\n\n", "#".repeat(*level as usize), parse::atx_closing(text)));
    }
    doc.push_str(edited.trim_start_matches('\n'));
}

/// What rewriting a unit needs: the index, the anchors of each included note, and the
/// images collected so far.
struct Assembly<'a> {
    index: &'a Index<'a>,
    anchors: BTreeMap<&'a VaultPath, Anchors>,
    assets: Assets,
}

impl Assembly<'_> {
    /// The text of unit `u` with its headings set to their levels, its block ids dropped
    /// and its links rewritten.
    fn unit_text(&mut self, u: &Unit) -> String {
        let note = u.note;
        let headings = u.headings();
        let links = u.links(self.index);
        let link_spans: Vec<_> = links.iter().map(|(l, _)| l.span.clone()).collect();
        let mut edits: Vec<Edit> =
            headings.iter().flat_map(|h| heading_edits(&note.text, h, u.level(h), &link_spans)).collect();

        // Block ids are vault plumbing: dropped with all the whitespace before them, line
        // breaks included (an id ends its block, so no line joins the next). When the line
        // left behind would change the note's block shape (`===` alone is a setext
        // underline), only the `^id` is replaced, by an empty HTML comment: the line stays
        // text and renders as nothing.
        for id in note.block_ids().map(|b| b.id).filter(|id| u.contains(&id.span)) {
            let start = note.text[..id.span.start].trim_end_matches([' ', '\t', '\r', '\n']).len();
            let mut left = note.text.clone();
            left.replace_range(start..id.span.end, "");
            if parse::Markdown::parse(left).same_shape(note) {
                edits.push(Edit::replace(start..id.span.end, ""));
            } else {
                edits.push(Edit::replace(id.span.clone(), "<!-- -->"));
            }
        }

        for (link, resolved) in links {
            let new = self.link_replacement(note, link, resolved);
            // In a setext heading the link goes on the heading's one line too.
            let inside =
                |h: &&Spanned<&Heading>| h.content.span.start <= link.span.start && link.span.end <= h.content.span.end;
            let in_setext = headings.iter().filter(inside).any(|h| h.setext);
            edits.push(Edit::replace(
                link.span.clone(),
                if in_setext { parse::one_line(&new).into_owned() } else { new },
            ));
        }

        // A heading reads back whole: when its text after the edits above ends in a `#`
        // run that ATX would take for closing `#`s, and it has none of its own, a closing
        // ` #` follows it (a setext heading's underline is gone).
        let closings: Vec<_> = headings
            .iter()
            .filter(|h| h.setext || note.text[h.content.span.end..h.span.end].trim().is_empty())
            .map(|h| Edit::insert(h.content.span.end, parse::atx_closing(&edited(&note.text, &h.content.span, &edits))))
            .collect();
        edits.extend(closings);

        // The text around the unit goes too; the edits above lie inside it.
        edits.push(Edit::replace(0..u.range.start, ""));
        edits.push(Edit::replace(u.range.end..note.text.len(), ""));
        splice::apply(&note.text, edits).expect("heading, block id and link edits never overlap")
    }

    /// What `link` in `note` becomes in the document: an image under `assets/`, a link
    /// to an included note's anchor, or plain text (its label, heading or title; for a
    /// link that does not resolve, its target as written) for anything left out. A
    /// label, heading or title is markdown and goes in as written; a target, path or file
    /// name is literal and goes in escaped ([`escape`]).
    fn link_replacement(&mut self, note: &Note, link: &Spanned<Link>, resolved: Option<Resolved<'_>>) -> String {
        let label = link.label.as_ref().map(|l| l.value(&note.text).to_string());
        let linked = match resolved {
            None => return label.unwrap_or_else(|| escape(link.target.text(&note.text))),
            Some(Resolved::File(p)) if link.form == LinkForm::Embed && is_image(p) => {
                let name = self.assets.name(p);
                let alt = label.filter(|l| !is_size(l)).unwrap_or_else(|| escape(&name));
                return format!("![{alt}]({ASSETS}/{})", percent_encoded(&name));
            }
            Some(Resolved::File(p)) => return label.unwrap_or_else(|| escape(p.as_str())),
            Some(Resolved::Note(n, _)) => n,
        };

        let heading = match &link.target.fragment {
            Some(Fragment::Heading(h)) => Some(h.as_str()),
            Some(Fragment::Block(_)) | None => None,
        };

        let text = label.unwrap_or_else(|| heading.map_or_else(|| linked.title().into_owned(), str::to_string));
        let anchor = self
            .anchors
            .get(&linked.path)
            .and_then(|a| heading.and_then(|h| a.headings.get(&heading_key(h))).or(a.top.as_ref()));

        match anchor {
            Some(id) => format!("[{text}](#{id})"),
            None => text,
        }
    }
}

/// `s` with every byte but RFC 3986's unreserved characters as `%XX`: a markdown link
/// destination that holds no space, and a URL that names the file, not a query or an
/// encoded name.
fn percent_encoded(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// `s` as markdown that reads as `s`: a backslash before each ASCII punctuation
/// character, which CommonMark lets any of them take (§2.4).
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Step 1 (m1-m4 §4): each note once, at its first appearance, breadth-first from
/// `root` up to `depth` links away.
fn collect<'a>(index: &'a Index, root: &'a Note, depth: usize) -> Vec<Unit<'a>> {
    let mut units = vec![Unit::root(root)];
    let mut seen: BTreeSet<&VaultPath> = BTreeSet::from([&root.path]);
    // (index into `units`, links from the root)
    let mut queue = VecDeque::from([(0usize, 0usize)]);
    while let Some((unit, distance)) = queue.pop_front() {
        if distance >= depth {
            continue;
        }

        // The notes the unit links to, each placed one level below the heading before
        // its link; read before any is added, since adding moves `units`.
        let parent = &units[unit];
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
            if !seen.insert(&note.path) {
                continue;
            }
            queue.push_back((units.len(), distance + 1));
            units.push(Unit::child(note, part, level));
        }
    }

    units
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
/// lines are also joined on one line ([`parse::line_ends`] as spaces, except inside a
/// link in `links`, whose replacement joins its own) and its underline dropped.
fn heading_edits(text: &str, h: &Spanned<&Heading>, level: u8, links: &[Range<usize>]) -> Vec<Edit> {
    let marks = "#".repeat(level as usize);
    let prefix = if h.content.span.is_empty() { marks } else { format!("{marks} ") };
    let mut edits = vec![Edit::replace(h.span.start..h.content.span.start, prefix)];
    if h.setext {
        let start = h.content.span.start;
        let ends = parse::line_ends(h.content.value(text)).map(|e| start + e.start..start + e.end);
        let in_link = |e: &Range<usize>| links.iter().any(|l| l.start < e.end && e.start < l.end);
        edits.extend(ends.filter(|e| !in_link(e)).map(|e| Edit::replace(e, " ")));
        let end = h.span.start + text[h.span.clone()].trim_end_matches(['\n', '\r']).len();
        edits.push(Edit::replace(h.content.span.end..end, ""));
    }
    edits
}

/// The bytes of `text` at `range` with the edits that lie inside it applied.
fn edited(text: &str, range: &Range<usize>, edits: &[Edit]) -> String {
    let inside = edits
        .iter()
        .filter(|e| range.start <= e.range.start && e.range.end <= range.end)
        .map(|e| Edit::replace(e.range.start - range.start..e.range.end - range.start, e.new.clone()))
        .collect();
    splice::apply(&text[range.clone()], inside).expect("edits inside one heading never overlap")
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
    fn image_destinations_name_the_file_as_a_url() {
        let name = "a <b> (1)\\&copy;%20?.png";
        let i = crate::testing::index(&[
            ("views/v.md", format!("# V\n\n![[views/{name}]]\n").as_str()),
            (format!("views/{name}").as_str(), ""),
        ]);
        let md = export(&i, "views/v.md", 0).markdown;
        let dests: Vec<_> = pulldown_cmark::Parser::new(&md)
            .filter_map(|e| match e {
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { dest_url, .. }) => Some(dest_url.to_string()),
                _ => None,
            })
            .collect();
        // Percent-encoded, so a browser loads this file, not `a <b> (1)\&copy; ?.png` or `a <b> (1)\&copy;%20`.
        assert_eq!(dests, ["assets/a%20%3Cb%3E%20%281%29%5C%26copy%3B%2520%3F.png"], "{md}");
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
        // A target is literal (escaped); a label is markdown (as written).
        let i = crate::testing::index(&[("views/v.md", "# V\n\nSee [[a_b_c*]], [[nope|*that*]] and [[#Missing]].\n")]);
        assert_eq!(export(&i, "views/v.md", 1).markdown, "# V\n\nSee a\\_b\\_c\\*, *that* and \\#Missing.\n");
        // What a link at a heading's end becomes may end in `#`: it gets a closing ` #`,
        // unless the heading has closing `#`s of its own.
        let i = crate::testing::index(&[(
            "views/v.md",
            "# V\n\n## See [[gone|Bug #]]\n\n## Kept [[gone|Bug #]] ##\n\nSetext [[gone|C #]]\n---\n",
        )]);
        let md = export(&i, "views/v.md", 0).markdown;
        assert!(
            md.contains("## See Bug # #\n") && md.contains("## Kept Bug # ##\n") && md.contains("## Setext C # #\n"),
            "{md}"
        );
        let read = parse::Markdown::parse(md);
        let texts: Vec<_> = read.headings().map(|h| h.content.value(&read.text).to_string()).collect();
        assert_eq!(texts, ["V", "See Bug #", "Kept Bug #", "Setext C #"]);
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

        // An inserted title reads back whole, on one line.
        let i = crate::testing::index(&[
            ("views/v.md", &format!("# V\n\nSee [[bfs#^{id}]].\n")),
            ("concepts/a/bfs.md", &format!("Lang\nmore #\n===\n\nThe key step. ^{id}\n")),
        ]);
        let md = export(&i, "views/v.md", 1).markdown;
        assert!(md.contains("See [Lang more #](#lang-more-).") && md.contains("## Lang more # #\n"), "{md}");
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
            assert_eq!(crate::parse::Markdown::parse(md.clone()).headings().count(), 1, "{md}");
        }
        for (body, kept) in [
            // No block id in a block quote (§4.6): the text stays.
            ("> Some text\n> === ^b1\n", "> === ^b1\n"),
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
            ("See [[views/v|a\nb]] here\n===\n", "# See [a b](#v) here\n"),
            ("Issue #\n---\n", "## Issue # #\n"),
            ("C#\n---\n", "## C#\n"),
            ("Lang\n#######\n---\n", "## Lang ####### #\n"),
            ("> First line\n> second line\n> ===\n", "> First line\n> second line\n> ===\n"),
            ("- Item line\n  more\n  ---\n", "- Item line\n  more\n  ---\n"),
        ] {
            let i = crate::testing::index(&[("views/v.md", format!("# V\n\n{body}"))]);
            let md = export(&i, "views/v.md", 0).markdown;
            assert!(md.contains(want), "{body:?}: {md:?}");
            let texts = |m: parse::Markdown| -> Vec<String> {
                m.headings().map(|h| h.heading_text(&m.text).into_owned()).collect()
            };
            if !body.contains("[[") {
                let (got, was) =
                    (texts(parse::Markdown::parse(md)), texts(parse::Markdown::parse(format!("# V\n\n{body}"))));
                assert_eq!(got, was, "{body:?}: the heading texts survive");
            }
        }
    }
}
