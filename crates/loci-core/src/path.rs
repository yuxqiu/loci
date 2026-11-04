//! Vault paths (m0 §3): the validated type every vault-relative path is held in.

use std::borrow::Borrow;
use std::fmt;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use relative_path::{Component, RelativePath};
use unicode_normalization::UnicodeNormalization;

use crate::config;
use crate::envelope::Refusal;
use crate::id::{SLUG_PATTERN, Slug};
use crate::lint::Code;

/// What a vault file is (m0 §3), by where it is ([`VaultPath::kind`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Concept,
    Area,
    View,
    Journal,
    Rules,
    Source,
    Inbox,
    /// A non-markdown file outside `sources/` and `inbox/` (images, attachments).
    Asset,
    /// A `.md` file outside every known folder.
    Unknown,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Kind::Concept => "concept",
            Kind::Area => "area",
            Kind::View => "view",
            Kind::Journal => "journal",
            Kind::Rules => "rules",
            Kind::Source => "source",
            Kind::Inbox => "inbox",
            Kind::Asset => "asset",
            Kind::Unknown => "unknown",
        })
    }
}

impl Kind {
    /// The formed notes, written to be read (concepts, area files, views, journal
    /// entries), as opposed to sources, inbox drafts and rule files.
    pub fn is_formed(self) -> bool {
        matches!(self, Kind::Concept | Kind::Area | Kind::View | Kind::Journal)
    }

    /// The managed kinds (m0 §3): C, A, V, J, R.
    pub fn is_managed(self) -> bool {
        matches!(self, Kind::Concept | Kind::Area | Kind::View | Kind::Journal | Kind::Rules)
    }
}

/// The rule files at the vault root (m0 §3): the agent entry point, the user's rules
/// and the user's style.
pub const AGENTS: &str = "AGENTS.md";
pub const RULES: &str = "RULES.md";
pub const STYLE: &str = "STYLE.md";

/// An area's scope note, `concepts/<area>/AREA.md` (m0 §6.3).
pub const AREA: &str = "AREA.md";

/// A path argument given by the user (m0 §8), normalized by the `relative-path` crate:
/// `.` and repeated or trailing `/` dropped, each `..` stepping back one folder; then
/// put in NFC. Resolving `..` by name is exact because a vault holds no symlinks (§3).
/// Refuses an absolute path, or one whose `..` climbs above the vault root
/// (`path.outside`), one that names the root itself (`path.root`), and one holding a
/// control character (`path.control`).
pub fn normalize_arg(p: &str) -> Result<String, Refusal> {
    let outside = || Refusal::refused("path.outside", format!("`{p}` leads outside the vault"));
    if Path::new(p).is_absolute() {
        return Err(outside());
    }
    // `normalize` keeps a `..` it cannot resolve at the front.
    let path = RelativePath::new(p).normalize();
    if path.components().next() == Some(Component::ParentDir) {
        return Err(outside());
    }
    if path.as_str().is_empty() {
        return Err(Refusal::refused("path.root", format!("`{p}` is the vault root, not a file or folder in it")));
    }
    if has_control(p) {
        return Err(Refusal::refused("path.control", format!("{p:?} has a control character")));
    }
    Ok(nfc(path.as_str()))
}

/// NFC form of a string.
pub fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// A file or folder in the vault, named exactly as it is: vault-relative, extension
/// included, so a note's path ends in `.md`. How a user names a note (`.md` left off,
/// or a concept slug) is resolved to one of these ([`crate::index::Index::note_named`]).
/// `/`-separated UTF-8, with no empty, `.` or `..` segment, no leading or trailing `/`,
/// and no control character (so every message that names one stays on one line). Not
/// necessarily NFC: git lists the names as stored, and lint reports a non-NFC one
/// (`path.not-nfc`). Ordered by bytes, as outputs are.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct VaultPath(String);

impl From<VaultPath> for String {
    fn from(p: VaultPath) -> String {
        p.0
    }
}

/// Read from JSON (a plugin's envelope) only if it is a vault path.
impl TryFrom<String> for VaultPath {
    type Error = String;

    fn try_from(s: String) -> Result<Self, String> {
        Self::new(s.clone()).ok_or_else(|| format!("`{s}` is not a vault path"))
    }
}

static SOURCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"^sources/{SLUG_PATTERN}/{SLUG_PATTERN}\.([a-z0-9]*[a-z][a-z0-9]*)$")).unwrap()
});

impl VaultPath {
    /// A path loci itself names (`loci.toml`, a plugin's files), a vault path by
    /// construction.
    pub fn fixed(s: &'static str) -> Self {
        Self::new(s.to_string()).unwrap_or_else(|| panic!("`{s}` is a vault path"))
    }

    /// `s` as a vault path, if it already is one; nothing is normalized.
    pub fn new(s: String) -> Option<Self> {
        let rel = RelativePath::new(&s);
        let normal =
            !s.is_empty() && rel.normalize().as_str() == s && rel.components().next() != Some(Component::ParentDir);
        (normal && !has_control(&s)).then_some(Self(s))
    }

    /// A path argument given by the user, as a vault path ([`normalize_arg`]).
    pub fn from_arg(p: &str) -> Result<Self, Refusal> {
        Ok(Self::new(normalize_arg(p)?).expect("a normalized path that neither is empty nor climbs out"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn rel(&self) -> &RelativePath {
        RelativePath::new(&self.0)
    }

    /// The `/`-separated segments.
    pub fn components(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.rel().components().map(Component::as_str)
    }

    /// The folder holding this path; `None` at the vault root.
    pub fn parent(&self) -> Option<VaultPath> {
        let parent = self.rel().parent().expect("a vault path is not empty");
        Self::new(parent.as_str().to_string())
    }

    /// The last segment.
    pub fn file_name(&self) -> &str {
        self.rel().file_name().expect("a vault path ends in a name")
    }

    /// The first `k` segments, if the path has that many (`k` > 0).
    pub fn head(&self, k: usize) -> Option<VaultPath> {
        let parts: Vec<&str> = self.components().take(k).collect();
        (k > 0 && parts.len() == k).then(|| Self(parts.join("/")))
    }

    /// Whether this path lies inside folder `folder` (not the folder itself).
    pub fn starts_with_folder(&self, folder: &VaultPath) -> bool {
        self != folder && self.rel().starts_with(folder.rel())
    }

    /// This path, inside folder `from`, moved into folder `to`; `None` when it is not
    /// inside `from`.
    pub fn with_prefix_replaced(&self, from: &VaultPath, to: &VaultPath) -> Option<VaultPath> {
        let rest = self.rel().strip_prefix(from.rel()).ok().filter(|r| !r.as_str().is_empty())?;
        Some(Self::new(to.rel().join(rest).into_string()).expect("a vault folder joined with vault segments"))
    }

    /// The kind of the file at this path. `None` for paths core never reads as vault
    /// files (m0 §3): `plugins/`, `.git/`, the root `loci.toml` and `.gitignore`.
    pub fn kind(&self) -> Option<Kind> {
        let parts: Vec<&str> = self.components().collect();

        // Plugin data, git's own files and the vault's configuration are never read as
        // vault files (m0 §3).
        if matches!(parts[0], "plugins" | ".git") || matches!(self.as_str(), config::FILE | ".gitignore") {
            return None;
        }

        Some(match parts.as_slice() {
            ["sources", ..] => Kind::Source,
            ["inbox", ..] => Kind::Inbox,
            _ if !self.is_note() => Kind::Asset,
            [AGENTS | RULES | STYLE] => Kind::Rules,
            ["concepts", _, AREA] => Kind::Area,
            ["concepts", _, _, ..] => Kind::Concept,
            ["views", _, ..] => Kind::View,
            ["journal", _, ..] => Kind::Journal,
            _ => Kind::Unknown,
        })
    }

    /// Whether core reads this path (it has a [`VaultPath::kind`]).
    pub fn is_core(&self) -> bool {
        self.kind().is_some()
    }

    /// The area of a path under `concepts/`.
    pub fn area(&self) -> Option<&str> {
        let mut parts = self.components();
        (parts.next() == Some("concepts")).then(|| parts.next()).flatten()
    }

    /// Whether this is a markdown note: its extension is `md` (a file named `.md` has
    /// none, so is not one).
    pub fn is_note(&self) -> bool {
        self.rel().extension() == Some("md")
    }

    /// A note's name: the file name without `.md` (a concept's slug); any other file's
    /// full name.
    pub fn stem(&self) -> &str {
        let p = self.rel();
        let name = if self.is_note() { p.file_stem() } else { p.file_name() };
        name.expect("a vault path ends in a name")
    }

    /// The slug a concept file names: its stem, if that is a slug (§6.2; an upper-case
    /// stem is none).
    pub fn slug(&self) -> Option<Slug> {
        self.stem().parse().ok()
    }

    /// Where this file is, parsed from its path with what the layout gives it, or the
    /// path rule it breaks (m0 §3, §6.2, §6.4, §6.6): lint reports one; `new` and `mv`
    /// refuse a destination that breaks one, with the same code. `Ok(None)` for a path
    /// core never reads ([`VaultPath::kind`]).
    pub fn place(&self) -> Result<Option<Place<'_>>, PathProblem> {
        let Some(kind) = self.kind() else { return Ok(None) };
        let parts: Vec<&str> = self.components().collect();
        Ok(Some(match (kind, parts.as_slice()) {
            (Kind::Concept, [_, area, ..]) => {
                Place::Concept { area, slug: self.stem().parse().map_err(PathProblem::SlugFormat)? }
            }
            (Kind::Area, [_, area, _]) => Place::Area { area },
            (Kind::View, _) => Place::View,
            (Kind::Journal, ["journal", stream, _, ..]) => {
                Place::Journal { stream: stream.parse().map_err(|_| PathProblem::JournalPath)? }
            }
            (Kind::Journal, _) => return Err(PathProblem::JournalPath),
            (Kind::Source, _) => {
                let ext = SOURCE.captures(&self.0).ok_or(PathProblem::SourcesLayout)?;
                Place::Source { ext: ext.get(1).expect("the extension group").as_str() }
            }
            (Kind::Inbox, ["inbox", folder, _, ..]) => Place::Inbox(InboxPlace::Folder(folder)),
            (Kind::Inbox, _) => Place::Inbox(InboxPlace::Root),
            (Kind::Rules, _) => Place::Rules,
            (Kind::Asset, _) => Place::Asset,
            (Kind::Unknown, _) => return Err(PathProblem::KindUnknown),
            (Kind::Concept | Kind::Area, _) => unreachable!("`kind` matched the folder layout"),
        }))
    }

    /// Why this path, which is not a concept path, cannot hold a concept: `new` and `mv`
    /// refuse with it (`new.no-area`, `mv.no-area`).
    pub fn why_not_concept(&self) -> &'static str {
        match self.kind() {
            Some(Kind::Area) => "would be the area file, not a concept",
            _ => "is not inside an area",
        }
    }
}

/// Whether `s` holds a control character (a newline, a tab, …), which no vault path may.
pub fn has_control(s: &str) -> bool {
    s.chars().any(char::is_control)
}

/// Where a validly placed vault file is (m0 §3), with what its path gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place<'a> {
    Concept {
        area: &'a str,
        slug: Slug,
    },
    Area {
        area: &'a str,
    },
    View,
    Journal {
        stream: Slug,
    },
    /// A source file `sources/<type>/<slug>.<ext>`.
    Source {
        ext: &'a str,
    },
    Inbox(InboxPlace<'a>),
    Rules,
    Asset,
}

/// Where a file in `inbox/` is (m0 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InboxPlace<'a> {
    /// Directly in `inbox/`.
    Root,
    /// Under `inbox/<folder>/`.
    Folder(&'a str),
}

/// A path rule a path breaks (m0 §3, §6.2, §6.4, §6.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathProblem {
    /// A file under `sources/` that is not `sources/<type>/<slug>.<ext>` (§6.4).
    SourcesLayout,
    /// A `.md` file outside every known folder (§3).
    KindUnknown,
    /// A concept whose stem is no slug (§6.2), and why.
    SlugFormat(String),
    /// A journal entry outside a `journal/<stream>/` folder named by a slug (§6.6).
    JournalPath,
}

impl PathProblem {
    /// The refusal of a destination at `path` that breaks this rule (`new`, `mv`).
    pub fn refusal(&self, path: &VaultPath) -> Refusal {
        Refusal::refused(self.code(), format!("`{path}`: {self}"))
    }

    pub fn code(&self) -> Code {
        match self {
            PathProblem::SourcesLayout => Code::SourcesLayout,
            PathProblem::KindUnknown => Code::KindUnknown,
            PathProblem::SlugFormat(_) => Code::SlugFormat,
            PathProblem::JournalPath => Code::JournalPath,
        }
    }
}

/// The message, which does not name the path: each caller does.
impl fmt::Display for PathProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathProblem::SourcesLayout => f.write_str("not a `sources/<type>/<slug>.<ext>` file"),
            PathProblem::KindUnknown => f.write_str("file is outside every known folder"),
            PathProblem::SlugFormat(why) => write!(f, "slug {why}"),
            PathProblem::JournalPath => {
                f.write_str("journal entry is not inside a `journal/<stream>/` folder whose name is a slug")
            }
        }
    }
}

impl fmt::Display for VaultPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Borrow<str> for VaultPath {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl PartialEq<&str> for VaultPath {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp(s: &str) -> VaultPath {
        VaultPath::new(s.to_string()).unwrap()
    }

    #[test]
    fn invariant() {
        for bad in ["", "/a", "a//b", "a/./b", "a/../b", "a/", ".", "..", "../a", "./a", "a\nb", "a\tb"] {
            assert!(VaultPath::new(bad.to_string()).is_none(), "{bad:?}");
        }
        for good in ["a", "a/b", "views/caf\u{65}\u{301}.md", ".gitignore", "a/.b/c..d"] {
            assert_eq!(VaultPath::new(good.to_string()).unwrap().as_str(), good);
        }
    }

    #[test]
    fn ordered_by_bytes() {
        assert!(vp("a-b") < vp("a/b"));
        assert!(vp("a/b") < vp("a0"));
        assert_eq!(serde_json::to_string(&vp("views/v.md")).unwrap(), "\"views/v.md\"");
    }

    #[test]
    fn user_paths() {
        // `.`, repeated and trailing `/` are only spelling; `..` steps back a folder.
        for (given, path) in [
            ("./views/x.md/", "views/x.md"),
            ("a/./b", "a/b"),
            ("a//b", "a/b"),
            ("./a", "a"),
            ("a/../b", "b"),
            ("views/caf\u{65}\u{301}.md", "views/caf\u{e9}.md"),
        ] {
            assert_eq!(VaultPath::from_arg(given).unwrap().as_str(), path, "{given}");
        }

        for (bad, code) in [
            ("/etc", "path.outside"),
            ("..", "path.outside"),
            ("a/../../b", "path.outside"),
            ("", "path.root"),
            ("views/a\nb.md", "path.control"),
            ("./", "path.root"),
            ("a/..", "path.root"),
        ] {
            assert_eq!(VaultPath::from_arg(bad).unwrap_err().code(), code, "{bad}");
        }
    }

    #[test]
    fn notes() {
        assert!(vp("views/v.md").is_note() && !vp("views/.md").is_note() && !vp("a/x.png").is_note());
    }

    #[test]
    fn kinds() {
        let kind = |p: &str| vp(p).kind();
        assert_eq!(kind("concepts/algo/bfs.md"), Some(Kind::Concept));
        assert_eq!(kind("concepts/algo/graphs/bfs.md"), Some(Kind::Concept));
        assert_eq!(kind("concepts/algo/AREA.md"), Some(Kind::Area));
        assert_eq!(kind("concepts/algo/graphs/AREA.md"), Some(Kind::Concept));
        assert_eq!(kind("concepts/x.md"), Some(Kind::Unknown));
        assert_eq!(kind("views/learn/bfs.md"), Some(Kind::View));
        assert_eq!(kind("journal/research/2025/03/25-03-14.md"), Some(Kind::Journal));
        assert_eq!(kind("RULES.md"), Some(Kind::Rules));
        assert_eq!(kind("AGENTS.md"), Some(Kind::Rules));
        assert_eq!(kind("notes.md"), Some(Kind::Unknown));
        assert_eq!(kind("sources/papers/x/paper.pdf"), Some(Kind::Source));
        assert_eq!(kind("inbox/drafts/a.md"), Some(Kind::Inbox));
        assert_eq!(kind("concepts/algo/attachments/x.png"), Some(Kind::Asset));
        assert_eq!(kind("plugins/learn/reviews.jsonl"), None);
        assert_eq!(kind("loci.toml"), None);
        assert_eq!(kind(".gitignore"), None);
        assert_eq!(kind(".git/config"), None);
        assert_eq!(kind("bench/cases.jsonl"), Some(Kind::Asset));
        assert!(vp("views/x.md").is_core());
        assert!(!vp("plugins/learn/x").is_core());
    }

    #[test]
    fn names_and_areas() {
        assert_eq!(vp("concepts/algo/binary-search.md").stem(), "binary-search");
        assert_eq!(vp("concepts/algo/fig.png").stem(), "fig.png");
        assert_eq!(vp("concepts/algo/graphs/bfs.md").area(), Some("algo"));
        assert_eq!(vp("concepts/x.md").area(), Some("x.md"));
        assert_eq!(vp("views/x.md").area(), None);
        assert_eq!(vp("views/d/x.md").file_name(), "x.md");
        assert_eq!(vp("views/d/x.md").parent(), Some(vp("views/d")));
        assert_eq!(vp("views").parent(), None);
        assert_eq!(vp("a/b/c").components().collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(vp("a/b/c").head(2), Some(vp("a/b")));
        assert_eq!(vp("a/b/c").head(4), None);
    }

    #[test]
    fn folders() {
        assert!(vp("views/d/x.md").starts_with_folder(&vp("views/d")));
        assert!(!vp("views/d").starts_with_folder(&vp("views/d")));
        assert!(!vp("views/dx/x.md").starts_with_folder(&vp("views/d")));
        assert_eq!(vp("views/d/e/x.md").with_prefix_replaced(&vp("views/d"), &vp("a/b")), Some(vp("a/b/e/x.md")));
        assert_eq!(vp("views/d").with_prefix_replaced(&vp("views/d"), &vp("a")), None);
        assert_eq!(vp("views/dx").with_prefix_replaced(&vp("views/d"), &vp("a")), None);
    }

    #[test]
    fn places() {
        let place = |p: &str| vp(p).place().map(|pl| pl.map(|pl| format!("{pl:?}")));
        assert_eq!(place("concepts/a/bfs.md").unwrap().unwrap(), r#"Concept { area: "a", slug: Slug("bfs") }"#);
        assert_eq!(place("sources/papers/x.mp3").unwrap().unwrap(), r#"Source { ext: "mp3" }"#);
        assert_eq!(place("journal/life/2025-03-14 rent.md").unwrap().unwrap(), r#"Journal { stream: Slug("life") }"#);
        assert_eq!(place("inbox/drafts/x.md").unwrap().unwrap(), r#"Inbox(Folder("drafts"))"#);
        assert_eq!(place("plugins/learn/x.jsonl"), Ok(None), "not read by core");
        for good in ["sources/papers/dijkstra-1959.pdf", "sources/clippings/heaps.md", "journal/life/a/b/notes.md"] {
            assert!(place(good).is_ok(), "{good}");
        }
        for bad in [
            "sources/Papers/x.pdf",
            "sources/papers/x",
            "sources/papers/X.pdf",
            "sources/papers/p/p.pdf",
            "sources/papers/x.PDF",
            "sources/papers/vaswani.03762",
            "sources/docs/a.b c",
        ] {
            assert_eq!(vp(bad).place().unwrap_err(), PathProblem::SourcesLayout, "{bad}");
        }
        let code = |p: &str| vp(p).place().err().map(|p| p.code());
        assert_eq!(code("sources/x.pdf"), Some(Code::SourcesLayout));
        assert_eq!(code("notes.md"), Some(Code::KindUnknown));
        assert_eq!(code("concepts/a/Bad Name.md"), Some(Code::SlugFormat));
        assert_eq!(code("journal/x.md"), Some(Code::JournalPath), "outside a stream");
        assert_eq!(code("journal/My Stuff/2025/03/25-03-14.md"), Some(Code::JournalPath), "the stream is a slug");
        assert_eq!(code("views/anything.md"), None);
        assert_eq!(vp("concepts/a/AREA.md").why_not_concept(), "would be the area file, not a concept");
        assert_eq!(vp("views/x.md").why_not_concept(), "is not inside an area");
    }
}
