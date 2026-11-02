//! The vault model (m0 §3): discovery, file kinds and the working-tree walk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::config::{self, Config};
use crate::envelope::Refusal;

/// Serialized as its [`Kind::name`].
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

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Concept => "concept",
            Kind::Area => "area",
            Kind::View => "view",
            Kind::Journal => "journal",
            Kind::Rules => "rules",
            Kind::Source => "source",
            Kind::Inbox => "inbox",
            Kind::Asset => "asset",
            Kind::Unknown => "unknown",
        }
    }

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

impl serde::Serialize for Kind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

static SLUG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\p{Ll}\p{Lo}\p{N}]+(-[\p{Ll}\p{Lo}\p{N}]+)*$").unwrap());
/// A slug (m0 §6.2): lowercase letters, other letters and digits, in `-`-joined words.
pub fn is_slug(s: &str) -> bool {
    SLUG.is_match(s)
}

/// A ULID in canonical form (Crockford base32, uppercase, 26 characters).
pub fn is_ulid(s: &str) -> bool {
    // Canonical Crockford base32, 26 characters, at most 128 bits (so the first is
    // 0-7). Not `ulid::Ulid::from_string`: it accepts lowercase and drops the bits
    // past 128 without an error.
    s.len() == 26 && s.as_bytes()[0] <= b'7' && s.bytes().all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b))
}

/// A journal entry path (m0 §6.6): a file inside `journal/<stream>/`, the stream a
/// slug. Folders and file names below the stream are the user's.
pub fn journal_path_ok(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() >= 3 && parts[0] == "journal" && is_slug(parts[1])
}

/// NFC form of a string.
pub fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// A path argument given by the user, as a vault path: `/` separators, no leading `./`
/// or trailing `/`, NFC. Refuses a path that could leave the vault: an absolute path,
/// or a `.`, `..` or empty segment (`path.outside`, m0 §8).
pub fn path_arg(p: &str) -> Result<String, Refusal> {
    let trimmed = p.trim_start_matches("./").trim_end_matches('/');
    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed.split('/').any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(Refusal::refused("path.outside", format!("`{p}` is not a path inside the vault")));
    }
    Ok(nfc(trimmed))
}

/// The extension of a source file `sources/<type>/<slug>.<ext>` (m0 §6.4), where
/// `<ext>` is lowercase ASCII letters and digits with at least one letter (`pdf`,
/// `mp3`, not `03762`); `None` for any other path.
pub fn source_ext(path: &str) -> Option<&str> {
    let parts: Vec<&str> = path.split('/').collect();
    let [top, ty, name] = parts[..] else { return None };
    let (slug, ext) = name.rsplit_once('.')?;
    let ext_ok = ext.bytes().any(|b| b.is_ascii_lowercase())
        && ext.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    (top == "sources" && is_slug(ty) && is_slug(slug) && ext_ok).then_some(ext)
}

/// Whether `path` is a source file (see [`source_ext`]).
pub fn is_source_file(path: &str) -> bool {
    source_ext(path).is_some()
}

/// The kind of a vault-relative, `/`-separated path. `None` for paths core never reads
/// as vault files (m0 §3): `plugins/`, `.git/`, the root `loci.toml` and `.gitignore`.
pub fn kind_of(path: &str) -> Option<Kind> {
    let parts: Vec<&str> = path.split('/').collect();
    let first = parts[0];
    // Plugin data, git's own files and the vault's configuration are never read as
    // vault files (m0 §3).
    if first == "plugins" || first == ".git" || path == config::FILE || path == ".gitignore" {
        return None;
    }
    let is_md = path.ends_with(".md");
    match first {
        "sources" => return Some(Kind::Source),
        "inbox" => return Some(Kind::Inbox),
        _ => {}
    }
    if !is_md {
        return Some(Kind::Asset);
    }
    if parts.len() == 1 {
        return Some(match first {
            "RULES.md" | "STYLE.md" | "AGENTS.md" => Kind::Rules,
            _ => Kind::Unknown,
        });
    }
    Some(match first {
        "concepts" if parts.len() == 3 && parts[2] == "AREA.md" => Kind::Area,
        "concepts" if parts.len() >= 3 => Kind::Concept,
        "views" => Kind::View,
        "journal" => Kind::Journal,
        _ => Kind::Unknown,
    })
}

/// A new concept id: a ULID (m0 §6.1).
pub fn new_concept_id() -> String {
    ulid::Ulid::generate().to_string()
}

/// The area of a path under `concepts/`.
pub fn area_of(path: &str) -> Option<&str> {
    let mut parts = path.split('/');
    (parts.next() == Some("concepts")).then(|| parts.next()).flatten()
}

/// A concept's slug: its file stem.
pub fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".md").unwrap_or(name)
}

#[derive(Debug, Clone)]
pub struct Vault {
    pub root: PathBuf,
    pub config: Config,
    /// The vault's files, listed once when it is opened.
    listing: Walk,
}

impl Vault {
    /// Finds the vault: `explicit`, or the nearest ancestor of `cwd` containing
    /// `loci.toml`. The root must be the root of a git work tree.
    /// Refuses with `vault.not-found`, `vault.not-git` or `config.invalid` (m0 §8).
    pub fn open(explicit: Option<&Path>, cwd: &Path) -> Result<Self, Refusal> {
        let not_found = |m: String| Refusal::refused("vault.not-found", m);
        let root = match explicit {
            Some(dir) => {
                let dir = if dir.is_absolute() { dir.to_path_buf() } else { cwd.join(dir) };
                if !dir.join(config::FILE).is_file() {
                    return Err(not_found(format!("{} has no loci.toml", dir.display())));
                }
                dir
            }
            None => cwd
                .ancestors()
                .find(|d| d.join(config::FILE).is_file())
                .map(Path::to_path_buf)
                .ok_or_else(|| not_found("no loci.toml in this directory or any parent".into()))?,
        };
        let root = root.canonicalize().map_err(|e| not_found(e.to_string()))?;
        match crate::git::toplevel(&root) {
            Ok(top) if top == root => {}
            Ok(_) => {
                return Err(Refusal::refused(
                    "vault.not-git",
                    format!("{} is not the root of a git work tree", root.display()),
                ));
            }
            Err(e) => {
                return Err(Refusal::refused(
                    "vault.not-git",
                    format!("{} is not in a git work tree: {e}", root.display()),
                ));
            }
        }
        // loci never reads or writes through a symlink, and reads every file it lists
        // (m0 §3). Checked here, once, so that no command has to: a vault breaking either
        // is refused before any command runs.
        let listing = walk(&root)?;
        let invalid = |m: String| Refusal::refused("config.invalid", m);
        let text = std::fs::read_to_string(root.join(config::FILE)).map_err(|e| invalid(e.to_string()))?;
        let config = Config::parse(&text).map_err(invalid)?;
        Ok(Self { root, config, listing })
    }

    pub fn abs(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// Every file core reads (m0 §3): the files git sees (tracked, or untracked and not
    /// ignored), minus `plugins/` and paths core never reads, as listed at open.
    pub fn listing(&self) -> &Walk {
        &self.listing
    }
}

/// The result of [`walk`].
#[derive(Debug, Clone, Default)]
pub struct Walk {
    /// Vault-relative `/` paths, sorted by bytes.
    pub files: Vec<String>,
    /// The text of every `.md` file in `files`, by path.
    pub markdown: BTreeMap<String, String>,
}

/// The first symlink on the way to the listed path `raw` (a folder above it, or the
/// path itself), as a vault path. Checked on the raw name, so a non-UTF-8 name is
/// checked too. loci never reads or writes through a symlink (m0 §3).
fn symlink_on_path(root: &Path, raw: &[u8]) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;
    let mut at = root.to_path_buf();
    let mut end = 0;
    for part in raw.split(|b| *b == b'/') {
        at.push(std::ffi::OsStr::from_bytes(part));
        end += part.len();
        if std::fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink()) {
            return Some(String::from_utf8_lossy(&raw[..end]).into_owned());
        }
        end += 1;
    }
    None
}

/// The path rule `path` breaks, if any, as (lint code, message): a file under `sources/`
/// that is not `sources/<type>/<slug>.<ext>` (§6.4), a `.md` file outside every known
/// folder (§3), a concept whose slug fails §6.2, a journal entry outside a
/// `journal/<stream>/` folder named by a slug (§6.6). Lint reports it; `new` and `mv`
/// refuse a destination that breaks one, with the same code. The message does not name
/// the path: each caller does.
pub fn path_problem(path: &str) -> Option<(&'static str, String)> {
    if path.starts_with("sources/") && !is_source_file(path) {
        return Some(("sources.layout", "not a `sources/<type>/<slug>.<ext>` file".to_string()));
    }
    match kind_of(path) {
        Some(Kind::Unknown) => Some(("kind.unknown", "file is outside every known folder".to_string())),
        Some(Kind::Concept) if !is_slug(stem(path)) => {
            Some(("slug.format", format!("slug `{}` does not match the slug pattern", stem(path))))
        }
        Some(Kind::Journal) if !journal_path_ok(path) => Some((
            "journal.path",
            "journal entry is not inside a `journal/<stream>/` folder whose name is a slug".to_string(),
        )),
        _ => None,
    }
}

/// Why `path`, which is not a concept path, cannot hold a concept: `new` and `mv` refuse
/// with it (`new.no-area`, `mv.no-area`).
pub fn why_not_concept(path: &str) -> &'static str {
    match kind_of(path) {
        Some(Kind::Area) => "would be the area file, not a concept",
        _ => "is not inside an area",
    }
}

/// Whether core reads vault path `rel` (it has a [`kind_of`]).
pub fn is_core_path(rel: &str) -> bool {
    kind_of(rel).is_some()
}

/// Whether vault path `rel` is a regular file (not a symlink, a folder or missing).
pub fn is_regular_file(root: &Path, rel: &str) -> bool {
    std::fs::symlink_metadata(root.join(rel)).is_ok_and(|m| m.is_file())
}

/// The files git sees (tracked, or untracked and not ignored: m0 §3) that exist on disk
/// and that core reads, with the text of each `.md` file. Git decides membership, as it
/// does for every other command. Refuses a failed listing (`git`), a vault in which any
/// listed path is, or lies under, a symlink (`file.symlink`), and one in which any file
/// core reads cannot be listed, read, or decoded as UTF-8, name included
/// (`file.unreadable`): loci never runs on a guess.
fn walk(root: &Path) -> Result<Walk, Refusal> {
    use std::os::unix::ffi::OsStrExt;
    let (paths, warnings) = crate::git::ls_files(root, None).map_err(Refusal::git)?;
    let mut out = Walk::default();
    let mut unreadable = vec![];
    // Git skips what it cannot read (a directory without permission) with a warning.
    // Wherever the folder is, `plugins/` included, loci cannot tell what it missed.
    unreadable.extend(warnings.lines().filter(|l| !l.trim().is_empty()).map(|l| format!("git: {}", l.trim())));
    let mut symlinks = vec![];
    for raw in paths {
        // Every listed path, `plugins/`, `loci.toml` and non-UTF-8 names included.
        if let Some(link) = symlink_on_path(root, &raw) {
            symlinks.push(format!("`{link}`"));
            continue;
        }
        // The prefixes `is_core_path` tests are ASCII, so a lossy name decides it.
        let name = String::from_utf8_lossy(&raw).into_owned();
        if !is_core_path(&name) {
            continue;
        }
        let meta = match std::fs::symlink_metadata(root.join(std::ffi::OsStr::from_bytes(&raw))) {
            Ok(m) => m,
            // A tracked file gone from disk, or whose folder became a file.
            Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => continue,
            Err(e) => {
                unreadable.push(format!("`{name}` cannot be read: {e}"));
                continue;
            }
        };
        if !meta.is_file() {
            continue; // A submodule.
        }
        let Ok(rel) = String::from_utf8(raw) else {
            unreadable.push(format!("`{name}` has a name that is not UTF-8"));
            continue;
        };
        if rel.ends_with(".md") {
            match std::fs::read(root.join(&rel)).map(String::from_utf8) {
                Ok(Ok(text)) => {
                    out.markdown.insert(rel.clone(), text);
                }
                Ok(Err(_)) => unreadable.push(format!("`{rel}` is not valid UTF-8")),
                Err(e) => unreadable.push(format!("`{rel}` cannot be read: {e}")),
            }
        } else if let Err(e) = std::fs::File::open(root.join(&rel)) {
            // Opened, not read: git could not hash it either, and would call it changed.
            unreadable.push(format!("`{rel}` cannot be read: {e}"));
        }
        out.files.push(rel);
    }
    if !symlinks.is_empty() {
        symlinks.sort();
        // Every file listed under a symlinked folder names the same link.
        symlinks.dedup();
        let message = listed("loci does not follow symlinks; replace each with the file or folder itself", &symlinks);
        return Err(Refusal::refused("file.symlink", message));
    }
    if !unreadable.is_empty() {
        unreadable.sort();
        unreadable.dedup();
        let message =
            listed("every file loci reads, and every folder git lists, must be readable; fix each", &unreadable);
        return Err(Refusal::refused("file.unreadable", message));
    }
    out.files.sort();
    // An unmerged file is listed once per conflict stage.
    out.files.dedup();
    Ok(out)
}

/// A refusal message listing `items`, one per line under `header`.
fn listed(header: &str, items: &[String]) -> String {
    let lines: Vec<String> = items.iter().map(|i| format!("\n  {i}")).collect();
    format!("{header}:{}", lines.concat())
}

/// Writes a file via a temporary sibling and a rename, so readers never see a partial file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    // A uniquely named file in the same folder, synced before it replaces `path`.
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(Path::new(".")))?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map(|_| ()).map_err(|e| e.error)
}

/// Copies the directory `from` to `to`, recursively, leaving out the entries of `from`
/// itself named in `skip`.
pub fn copy_dir(from: &Path, to: &Path, skip: &[&str]) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if skip.iter().any(|s| entry.file_name() == *s) {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if entry.file_type()?.is_dir() { copy_dir(&src, &dst, &[])? } else { std::fs::copy(&src, &dst).map(|_| ())? }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulids() {
        assert!(is_ulid("01JB9XQ7Z3K4M5N6P7Q8R9S0TV"));
        assert!(!is_ulid("abc"));
        assert!(!is_ulid("01jb9xq7z3k4m5n6p7q8r9s0tv"));
        assert!(!is_ulid("81JB9XQ7Z3K4M5N6P7Q8R9S0TV"), "above the 128-bit range");
        assert!(!is_ulid("01JB9XQ7Z3K4M5N6P7Q8R9S0TI"), "an alias, not the canonical form");
        assert!(!is_ulid("01JB9XQ7Z3K4M5N6P7Q8R9S0TI"));
    }

    #[test]
    fn source_files() {
        assert!(is_source_file("sources/papers/dijkstra-1959.pdf"));
        assert!(is_source_file("sources/clippings/heaps.md"));
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
            assert!(!is_source_file(bad), "{bad}");
        }
    }

    #[test]
    fn user_paths() {
        assert_eq!(path_arg("./views/x.md/").unwrap(), "views/x.md");
        for bad in ["", "/etc", "a/../b", "a/./b", "a//b", "..", "./"] {
            assert!(path_arg(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of("concepts/algo/bfs.md"), Some(Kind::Concept));
        assert_eq!(kind_of("concepts/algo/graphs/bfs.md"), Some(Kind::Concept));
        assert_eq!(kind_of("concepts/algo/AREA.md"), Some(Kind::Area));
        assert_eq!(kind_of("concepts/algo/graphs/AREA.md"), Some(Kind::Concept));
        assert_eq!(kind_of("concepts/x.md"), Some(Kind::Unknown));
        assert_eq!(kind_of("views/learn/bfs.md"), Some(Kind::View));
        assert_eq!(kind_of("journal/research/2025/03/25-03-14.md"), Some(Kind::Journal));
        assert_eq!(kind_of("RULES.md"), Some(Kind::Rules));
        assert_eq!(kind_of("AGENTS.md"), Some(Kind::Rules));
        assert_eq!(kind_of("notes.md"), Some(Kind::Unknown));
        assert_eq!(kind_of("sources/papers/x/paper.pdf"), Some(Kind::Source));
        assert_eq!(kind_of("inbox/drafts/a.md"), Some(Kind::Inbox));
        assert_eq!(kind_of("concepts/algo/attachments/x.png"), Some(Kind::Asset));
        assert_eq!(kind_of("plugins/learn/reviews.jsonl"), None);
        assert_eq!(kind_of("bench/cases.jsonl"), Some(Kind::Asset));
    }

    #[test]
    fn stems_and_areas() {
        assert_eq!(stem("concepts/algo/binary-search.md"), "binary-search");
        assert_eq!(area_of("concepts/algo/graphs/bfs.md"), Some("algo"));
        assert_eq!(area_of("views/x.md"), None);
    }
}
