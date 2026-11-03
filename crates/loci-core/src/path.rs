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

/// NFC form of a string.
pub fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// A vault-relative path: `/`-separated UTF-8, with no empty, `.` or `..` segment, no
/// leading or trailing `/`, and no control character (so every message that names one
/// stays on one line). Not necessarily NFC: git lists the names as stored, and
/// lint reports a non-NFC one (`path.not-nfc`). Ordered by bytes, as outputs are.
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

    /// A path argument given by the user, as a vault path, normalized by the
    /// `relative-path` crate: `.` and repeated or trailing `/` dropped, each `..` stepping
    /// back one folder; then put in NFC. Resolving `..` by name is exact because a vault
    /// holds no symlinks (§3). Refuses an absolute path, or one whose `..` climbs above
    /// the vault root (`path.outside`), and one that names the root itself (`path.root`)
    /// (m0 §8).
    pub fn from_arg(p: &str) -> Result<Self, Refusal> {
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
        Ok(Self::new(nfc(path.as_str())).expect("a normalized path that neither is empty nor climbs out"))
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

    /// This path as a note argument means it (m0 §8): itself if it is a note, else with
    /// `.md` added.
    pub fn to_note(&self) -> VaultPath {
        if self.is_note() {
            self.clone()
        } else {
            Self::new(format!("{}.md", self.0)).expect("a suffix keeps a vault path")
        }
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

    /// The extension of a source file `sources/<type>/<slug>.<ext>` (m0 §6.4): `<type>`
    /// and `<slug>` are slugs, `<ext>` lowercase ASCII letters and digits with at least
    /// one letter (`pdf`, `mp3`, not `03762`); `None` for any other path.
    pub fn source_ext(&self) -> Option<&str> {
        SOURCE.captures(&self.0).map(|c| c.get(1).expect("the extension group").as_str())
    }

    /// Whether this is a source file (see [`VaultPath::source_ext`]).
    pub fn is_source_file(&self) -> bool {
        self.source_ext().is_some()
    }

    /// Whether this is a valid journal entry path (m0 §6.6): a file inside
    /// `journal/<stream>/`, the stream a slug. Folders and file names below the stream
    /// are the user's.
    fn journal_ok(&self) -> bool {
        let parts: Vec<&str> = self.components().collect();
        parts.len() >= 3 && parts[0] == "journal" && parts[1].parse::<Slug>().is_ok()
    }

    /// The path rule this path breaks, if any. Lint reports it; `new` and `mv` refuse a
    /// destination that breaks one, with the same code.
    pub fn problem(&self) -> Option<PathProblem> {
        if self.0.starts_with("sources/") && !self.is_source_file() {
            return Some(PathProblem::SourcesLayout);
        }
        match self.kind() {
            Some(Kind::Unknown) => Some(PathProblem::KindUnknown),
            Some(Kind::Concept) => self.stem().parse::<Slug>().err().map(PathProblem::SlugFormat),
            Some(Kind::Journal) if !self.journal_ok() => Some(PathProblem::JournalPath),
            _ => None,
        }
    }

    /// Where in `inbox/` this file is: directly in it, or under one of its folders.
    /// `None` outside the inbox.
    pub fn inbox_place(&self) -> Option<InboxPlace<'_>> {
        let parts: Vec<&str> = self.components().collect();
        match parts.as_slice() {
            ["inbox", _] => Some(InboxPlace::Root),
            ["inbox", folder, _, ..] => Some(InboxPlace::Folder(folder)),
            _ => None,
        }
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
        assert_eq!(vp("concepts/a/x").to_note(), vp("concepts/a/x.md"));
        assert_eq!(vp("concepts/a/x.md").to_note(), vp("concepts/a/x.md"));
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
    fn source_files() {
        assert!(vp("sources/papers/dijkstra-1959.pdf").is_source_file());
        assert!(vp("sources/clippings/heaps.md").is_source_file());
        assert_eq!(vp("sources/papers/x.mp3").source_ext(), Some("mp3"));
        for bad in [
            "sources/Papers/x.pdf",
            "sources/papers/x",
            "sources/papers/X.pdf",
            "sources/x.pdf",
            "sources/papers/p/p.pdf",
            "sources/papers/x.PDF",
            "sources/papers/vaswani.03762",
            "sources/docs/a.b c",
        ] {
            assert!(!vp(bad).is_source_file(), "{bad}");
        }
    }

    #[test]
    fn journal_paths() {
        assert!(vp("journal/research/2025/03/25-03-14.md").journal_ok());
        assert!(vp("journal/life/2025-03-14 rent.md").journal_ok(), "names below the stream are free");
        assert!(vp("journal/life/a/b/notes.md").journal_ok());
        assert!(!vp("journal/x.md").journal_ok(), "outside a stream");
        assert!(!vp("journal/My Stuff/2025/03/25-03-14.md").journal_ok(), "the stream is a slug");
        assert!(!vp("journal/Research/2025/03/25-03-14.md").journal_ok(), "the stream is a slug");
    }

    #[test]
    fn problems() {
        let code = |p: &str| vp(p).problem().map(|p| p.code());
        assert_eq!(code("sources/x.pdf"), Some(Code::SourcesLayout));
        assert_eq!(code("notes.md"), Some(Code::KindUnknown));
        assert_eq!(code("concepts/a/Bad Name.md"), Some(Code::SlugFormat));
        assert_eq!(code("journal/x.md"), Some(Code::JournalPath));
        assert_eq!(code("concepts/a/good.md"), None);
        assert_eq!(code("views/anything.md"), None);
        assert_eq!(vp("concepts/a/AREA.md").why_not_concept(), "would be the area file, not a concept");
        assert_eq!(vp("views/x.md").why_not_concept(), "is not inside an area");
    }
}
