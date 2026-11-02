//! The vault model (m0 §3): discovery, file kinds and the working-tree walk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use relative_path::RelativePath;
use unicode_normalization::UnicodeNormalization;

use crate::config::{self, Config};
use crate::envelope::Refusal;
use crate::path::VaultPath;

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

/// The rule files at the vault root (m0 §3): the agent entry point, the user's rules
/// and the user's style.
pub const AGENTS: &str = "AGENTS.md";
pub const RULES: &str = "RULES.md";
pub const STYLE: &str = "STYLE.md";

/// NFC form of a string.
pub fn nfc(s: &str) -> String {
    s.nfc().collect()
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
        let why = match crate::git::toplevel(&root) {
            Ok(top) if top == root => None,
            Ok(_) => Some("is not the root of a git work tree".to_string()),
            Err(e) => Some(format!("is not in a git work tree: {e}")),
        };
        if let Some(why) = why {
            return Err(Refusal::refused("vault.not-git", format!("{} {why}", root.display())));
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

    /// Where vault path `rel` is on disk.
    pub fn abs(&self, rel: &VaultPath) -> PathBuf {
        abs(&self.root, rel)
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
    pub files: Vec<VaultPath>,
    /// The text of every `.md` file in `files`, by path.
    pub markdown: BTreeMap<VaultPath, String>,
}

/// The first symlink on the way to the listed path `name` (a folder above it, or the
/// path itself), as a vault path. loci never reads or writes through a symlink (m0 §3).
fn symlink_on_path(root: &Path, name: &str) -> Option<String> {
    let mut at = root.to_path_buf();
    let mut seen: Vec<&str> = vec![];
    for part in RelativePath::new(name).components() {
        at.push(part.as_str());
        seen.push(part.as_str());
        if std::fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink()) {
            return Some(seen.join("/"));
        }
    }
    None
}

/// Where vault path `rel` is on disk, under vault root `root`: the one place a vault
/// path becomes a file system path.
fn abs(root: &Path, rel: &VaultPath) -> PathBuf {
    root.join(rel.as_str())
}

/// Whether vault path `rel` is a regular file (not a symlink, a folder or missing).
pub fn is_regular_file(root: &Path, rel: &VaultPath) -> bool {
    std::fs::symlink_metadata(abs(root, rel)).is_ok_and(|m| m.is_file())
}

/// The files git sees (tracked, or untracked and not ignored: m0 §3) that exist on disk
/// and that core reads, with the text of each `.md` file. Git decides membership, as it
/// does for every other command. Refuses a failed listing (`git`), a vault in which any
/// listed path is, or lies under, a symlink (`file.symlink`), and one in which any file
/// core reads cannot be listed, read, or decoded as UTF-8, or in which any name git
/// lists, anywhere, is not UTF-8 (`file.unreadable`): loci never runs on a guess.
fn walk(root: &Path) -> Result<Walk, Refusal> {
    let (names, warnings) = crate::git::ls_files(root, None).map_err(Refusal::git)?;
    let mut out = Walk::default();
    let mut unreadable = vec![];
    // Git skips what it cannot read (a directory without permission) with a warning.
    // Wherever the folder is, `plugins/` included, loci cannot tell what it missed.
    unreadable.extend(warnings.lines().filter(|l| !l.trim().is_empty()).map(|l| format!("git: {}", l.trim())));
    let mut symlinks = vec![];
    for name in names {
        // Every listed name, `plugins/` and files gone from disk included.
        let name = match name {
            Ok(name) => name,
            Err(raw) => {
                unreadable.push(format!("`{}` has a name that is not UTF-8", String::from_utf8_lossy(&raw)));
                continue;
            }
        };
        // Every UTF-8 name, `plugins/` and `loci.toml` included.
        if let Some(link) = symlink_on_path(root, &name) {
            symlinks.push(format!("`{link}`"));
            continue;
        }
        let rel = crate::git::vault_path(name);
        if !rel.is_core() {
            continue;
        }
        let abs = abs(root, &rel);
        let meta = match std::fs::symlink_metadata(&abs) {
            Ok(m) => m,
            // A tracked file gone from disk, or whose folder became a file.
            Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => continue,
            Err(e) => {
                unreadable.push(format!("`{rel}` cannot be read: {e}"));
                continue;
            }
        };
        if !meta.is_file() {
            continue; // A submodule.
        }
        if rel.is_note() {
            match std::fs::read(&abs).map(String::from_utf8) {
                Ok(Ok(text)) => {
                    out.markdown.insert(rel.clone(), text);
                }
                Ok(Err(_)) => unreadable.push(format!("`{rel}` is not valid UTF-8")),
                Err(e) => unreadable.push(format!("`{rel}` cannot be read: {e}")),
            }
        } else if let Err(e) = std::fs::File::open(&abs) {
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
    format!("{header}:\n  {}", items.join("\n  "))
}

/// Writes a file via a temporary sibling and a rename, so readers never see a partial file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    // A uniquely named file in the same folder, synced before it replaces `path`.
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().expect("an absolute file path has a folder"))?;
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
