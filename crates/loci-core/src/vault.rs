//! The vault model (m0 §3): discovery, file kinds and the working-tree walk.

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
    s.len() == 26 && s.bytes().all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b))
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
/// (`plugins/`, `.git/`).
pub fn kind_of(path: &str) -> Option<Kind> {
    let parts: Vec<&str> = path.split('/').collect();
    let first = parts[0];
    if first == "plugins" || first == ".git" {
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
        let invalid = |m: String| Refusal::refused("config.invalid", m);
        let text = std::fs::read_to_string(root.join(config::FILE)).map_err(|e| invalid(e.to_string()))?;
        let config = Config::parse(&text).map_err(invalid)?;
        Ok(Self { root, config })
    }

    pub fn abs(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// Every file core reads (m0 §3): the working tree minus `.git/`, `plugins/` and
    /// gitignored paths.
    pub fn walk(&self) -> Walk {
        walk(&self.root)
    }
}

/// Paths core owns outside the vault content: `.git/`, `loci.toml` and the root
/// `.gitignore`. They are never read as vault files nor written by `mv`.
pub fn is_reserved(path: &str) -> bool {
    path == ".git" || path.starts_with(".git/") || path == config::FILE || path == ".gitignore"
}

/// The result of [`walk`].
#[derive(Debug, Default)]
pub struct Walk {
    /// Vault-relative `/` paths, sorted by bytes.
    pub files: Vec<String>,
    /// Paths that could not be listed or have a non-UTF-8 name, with the reason (`.`
    /// for the root).
    pub unreadable: Vec<(String, String)>,
}

fn walk(root: &Path) -> Walk {
    let mut out = Walk::default();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .ignore(false)
        .parents(false)
        .require_git(false)
        .filter_entry(|e| {
            let name = e.file_name();
            !(e.depth() == 1 && (name == ".git" || name == "plugins"))
        })
        .build();
    let relative = |p: &Path| {
        let rel = p.strip_prefix(root).expect("the walk stays under the root");
        let parts: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect();
        if parts.is_empty() { ".".to_string() } else { parts.join("/") }
    };
    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                let path = error_path(&e).map_or(".".to_string(), relative);
                out.unreadable.push((path, e.to_string()));
                continue;
            }
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let rel = relative(entry.path());
        if entry.path().to_str().is_none() {
            out.unreadable.push((rel, "file name is not UTF-8".to_string()));
            continue;
        }
        if is_reserved(&rel) {
            continue;
        }
        if kind_of(&rel).is_some() {
            out.files.push(rel);
        }
    }
    out.files.sort();
    out.unreadable.sort();
    out
}

/// The path an `ignore` walk error is about, if it names one.
fn error_path(e: &ignore::Error) -> Option<&Path> {
    match e {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => error_path(err),
        _ => None,
    }
}

/// Writes a file via a temporary sibling and a rename, so readers never see a partial file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("{}.loci-tmp", path.extension().and_then(|e| e.to_str()).unwrap_or("")));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulids() {
        assert!(is_ulid("01JB9XQ7Z3K4M5N6P7Q8R9S0TV"));
        assert!(!is_ulid("abc"));
        assert!(!is_ulid("01jb9xq7z3k4m5n6p7q8r9s0tv"));
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
        assert_eq!(kind_of("bench/retrieval.jsonl"), Some(Kind::Asset));
    }

    #[test]
    fn stems_and_areas() {
        assert_eq!(stem("concepts/algo/binary-search.md"), "binary-search");
        assert_eq!(area_of("concepts/algo/graphs/bfs.md"), Some("algo"));
        assert_eq!(area_of("views/x.md"), None);
    }
}
