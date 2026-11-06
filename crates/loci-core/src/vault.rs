//! The vault model (m0 §3): discovery, configuration, and the files git lists (read once,
//! at open).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::{self, Config};
use crate::envelope::Refusal;
use crate::path::VaultPath;
use crate::relink::Change;

/// Where the vault is (m0 §8, global options): the folder `--vault` gives, or the
/// nearest folder holding `loci.toml` from a folder up. Both absolute.
#[derive(Debug, Clone)]
pub enum VaultDir {
    Given(PathBuf),
    Search(PathBuf),
}

impl VaultDir {
    /// From `--vault` (relative to `cwd`), else a search from `cwd`.
    pub fn new(given: Option<&Path>, cwd: &Path) -> Self {
        match given {
            Some(dir) => Self::Given(cwd.join(dir)),
            None => Self::Search(cwd.to_path_buf()),
        }
    }

    /// The folder itself: the one given, or the one the search starts from.
    pub fn dir(&self) -> &Path {
        match self {
            Self::Given(dir) | Self::Search(dir) => dir,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Vault {
    root: PathBuf,
    config: Config,
    /// Every file core reads (m0 §3): the files git sees (tracked, or untracked and not
    /// ignored), minus `plugins/` and paths core never reads, listed once at open;
    /// sorted, each once.
    files: Vec<VaultPath>,
    /// The text of each note among `files`.
    texts: BTreeMap<VaultPath, String>,
}

impl Vault {
    /// Opens the vault `at` names. The root must be the root of a git work tree.
    /// Refuses with `vault.not-found`, `vault.not-git`, `file.symlink`, `file.unreadable`,
    /// `git` (exit 3) or `config.invalid` (m0 §8).
    pub fn open(at: &VaultDir) -> Result<Self, Refusal> {
        let not_found = |m: String| Refusal::refused("vault.not-found", m);
        let root = match at {
            VaultDir::Given(dir) => {
                if !dir.join(config::FILE).is_file() {
                    return Err(not_found(format!("{} has no loci.toml", dir.display())));
                }
                dir.clone()
            }
            VaultDir::Search(dir) => dir
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
        let (files, texts) = walk(&root)?;
        let invalid = |m: String| Refusal::refused("config.invalid", format!("{}: {m}", config::FILE));
        let text = std::fs::read_to_string(root.join(config::FILE)).map_err(|e| invalid(e.to_string()))?;
        let config = Config::parse(&text).map_err(invalid)?;
        Ok(Self::of(root, config, files, texts))
    }

    /// The tree `change` would leave, in memory: its moves and deletions done, its
    /// writes in place. Nothing is written.
    pub fn after(&self, change: &Change) -> Vault {
        let kept = |p: &&VaultPath| !change.deleted.contains(*p);
        let mut files: Vec<VaultPath> = self.files.iter().filter(kept).map(|p| change.final_path(p)).collect();
        files.sort();
        let text = |(p, t): (&VaultPath, &String)| {
            let p = change.final_path(p);
            let t = change.writes.get(&p).map_or_else(|| t.clone(), |w| w.text.clone());
            (p, t)
        };
        let texts = self.texts.iter().filter(|(p, _)| kept(p)).map(text).collect();
        Self::of(self.root.clone(), self.config.clone(), files, texts)
    }

    /// The vault of `files` (sorted, each once) and the notes' `texts`.
    pub(crate) fn of(
        root: PathBuf,
        config: Config,
        files: Vec<VaultPath>,
        texts: BTreeMap<VaultPath, String>,
    ) -> Vault {
        Vault { root, config, files, texts }
    }

    /// The vault's git work tree.
    pub fn repo(&self) -> crate::git::Repo<'_> {
        crate::git::Repo::new(&self.root)
    }

    /// Where vault path `rel` is on disk.
    pub fn abs(&self, rel: &VaultPath) -> PathBuf {
        abs(&self.root, rel)
    }

    /// Whether anything (a file, a folder, a symlink) is on disk at vault path `rel`.
    pub fn exists(&self, rel: &VaultPath) -> bool {
        self.abs(rel).symlink_metadata().is_ok()
    }

    /// The vault's folder, for running processes in it (git, plugins). A file in the
    /// vault is reached through [`Vault::abs`], never by joining onto this.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Writes `bytes` to vault path `rel` atomically (a temporary sibling, then a
    /// rename), creating its folders. A symlink on the way is refused (`file.symlink`):
    /// loci never writes through one (m0 §3), and folders git ignores were not checked
    /// when the vault was opened.
    pub fn write(&self, rel: &VaultPath, bytes: &[u8]) -> Result<(), Refusal> {
        self.refuse_symlink(rel)?;
        let path = self.abs(rel);
        let dir = path.parent().expect("a vault file has a folder");
        std::fs::create_dir_all(dir).map_err(Refusal::io_at(dir))?;
        write_atomic(&path, bytes).map_err(Refusal::io_at(&path))
    }

    /// The bytes of the file at vault path `rel`, `None` if there is none. A symlink on
    /// the way is refused (`file.symlink`), as for [`Vault::write`].
    pub fn read(&self, rel: &VaultPath) -> Result<Option<Vec<u8>>, Refusal> {
        self.refuse_symlink(rel)?;
        let path = self.abs(rel);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Refusal::io_at(&path)(e)),
        }
    }

    /// [`Vault::read`] as UTF-8 text; other bytes are `io`, naming the file.
    pub fn read_text(&self, rel: &VaultPath) -> Result<Option<String>, Refusal> {
        let invalid = |e| Refusal::io_at(&self.abs(rel))(std::io::Error::new(std::io::ErrorKind::InvalidData, e));
        self.read(rel)?.map(|b| String::from_utf8(b).map_err(invalid)).transpose()
    }

    /// Refuses a symlink on the way to vault path `rel` (`file.symlink`): loci never
    /// reads or writes through one (m0 §3).
    pub fn refuse_symlink(&self, rel: &VaultPath) -> Result<(), Refusal> {
        match symlink_on_path(&self.root, rel) {
            Some(link) => Err(Refusal::refused(
                "file.symlink",
                format!("`{link}` is a symlink, which loci does not read or write through"),
            )),
            None => Ok(()),
        }
    }

    /// The vault's `loci.toml`.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Every file core reads, sorted.
    pub fn files(&self) -> &[VaultPath] {
        &self.files
    }

    /// The text of `note`, one of the vault's notes.
    pub(crate) fn text(&self, note: &VaultPath) -> &str {
        &self.texts[note]
    }

    /// Whether `path` is one of the vault's files.
    pub fn contains(&self, path: &VaultPath) -> bool {
        self.files.binary_search(path).is_ok()
    }

    /// What a written path names (m0 §5.3): the file at exactly that path, else the note
    /// at that path with `.md` added.
    pub fn resolve(&self, written: &str) -> Option<&VaultPath> {
        let file = |path: &str| self.files.binary_search_by(|f| f.as_str().cmp(path)).ok().map(|i| &self.files[i]);
        file(written).or_else(|| file(&format!("{written}.md")))
    }
}

/// The first symlink on the way to vault path `rel` (a listed name, or a path about to
/// be written): a folder above it, or the path itself. loci never reads or writes
/// through a symlink (m0 §3).
fn symlink_on_path(root: &Path, rel: &VaultPath) -> Option<VaultPath> {
    (1..=rel.components().count())
        .filter_map(|k| rel.head(k))
        .find(|p| abs(root, p).symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()))
}

/// Where vault path `rel` is on disk, under vault root `root`: the one place a vault
/// path becomes a file system path.
fn abs(root: &Path, rel: &VaultPath) -> PathBuf {
    root.join(rel.as_str())
}

/// Whether vault path `rel` is a regular file (not a symlink, a folder or missing).
pub(crate) fn is_regular_file(root: &Path, rel: &VaultPath) -> bool {
    std::fs::symlink_metadata(abs(root, rel)).is_ok_and(|m| m.is_file())
}

/// The files git sees (tracked, or untracked and not ignored: m0 §3) that exist on disk
/// and that core reads, with the text of each `.md` file. Git decides membership, as it
/// does for every other command. Refuses a failed listing (`git`), a vault in which any
/// listed path is, or lies under, a symlink (`file.symlink`), and one in which any file
/// core reads cannot be listed, read, or decoded as UTF-8, or in which any name git
/// reports without `--ignored` (listed, or in HEAD's tree) is no vault path
/// (`file.unreadable`): loci never runs on a guess.
fn walk(root: &Path) -> Result<(Vec<VaultPath>, BTreeMap<VaultPath, String>), Refusal> {
    let listed = crate::git::Repo::new(root).ls_files().map_err(Refusal::git)?;

    // Git skips what it cannot read (a directory without permission) with a warning.
    // Wherever the folder is, `plugins/` included, loci cannot tell what it missed.
    let mut unreadable: Vec<String> = listed.warnings.iter().map(|w| format!("git: {w}")).collect();

    // Every name git reports, `plugins/`, files gone from disk and HEAD's included, must
    // be a vault path: no command handles another.
    unreadable.extend(listed.name_problems);

    let (mut files, mut texts) = (vec![], BTreeMap::new());
    let mut symlinks = vec![];
    for rel in listed.paths {
        // Every name, `plugins/` and `loci.toml` included, is checked for symlinks.
        if let Some(link) = symlink_on_path(root, &rel) {
            symlinks.push(format!("`{link}`"));
            continue;
        }
        match entry(root, &rel) {
            Entry::Skipped => {}
            Entry::Unreadable(why) => unreadable.push(format!("`{rel}` {why}")),
            Entry::Note(text) => {
                texts.insert(rel.clone(), text);
                files.push(rel);
            }
            Entry::File => files.push(rel),
        }
    }

    refuse_unsafe(symlinks, unreadable)?;
    files.sort();

    // An unmerged file is listed once per conflict stage.
    files.dedup();
    Ok((files, texts))
}

/// Refuses the vault when a listed path is under a symlink (`file.symlink`), or when a
/// file is unreadable (`file.unreadable`), naming each once.
fn refuse_unsafe(mut symlinks: Vec<String>, mut unreadable: Vec<String>) -> Result<(), Refusal> {
    if !symlinks.is_empty() {
        symlinks.sort();
        // Every file listed under a symlinked folder names the same link.
        symlinks.dedup();
        let message = bulleted("loci does not follow symlinks; replace each with the file or folder itself", &symlinks);
        return Err(Refusal::refused("file.symlink", message));
    }
    if !unreadable.is_empty() {
        unreadable.sort();
        unreadable.dedup();
        let message =
            bulleted("every file loci reads, and every folder git lists, must be readable; fix each", &unreadable);
        return Err(Refusal::refused("file.unreadable", message));
    }
    Ok(())
}

/// What a listed vault path is, for [`walk`].
enum Entry {
    /// A note core reads, with its text.
    Note(String),
    /// Any other file core reads.
    File,
    /// Not a file core reads: outside what core reads, gone from disk, or a folder.
    Skipped,
    /// Why core cannot read it (`file.unreadable`).
    Unreadable(String),
}

/// What vault path `rel` (not under a symlink) is on disk.
fn entry(root: &Path, rel: &VaultPath) -> Entry {
    if !rel.is_core() {
        return Entry::Skipped;
    }

    let abs = abs(root, rel);
    let meta = match std::fs::symlink_metadata(&abs) {
        Ok(m) => m,
        // A tracked file gone from disk, or whose folder became a file.
        Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => {
            return Entry::Skipped;
        }
        Err(e) => return Entry::Unreadable(format!("cannot be read: {e}")),
    };

    // A submodule, an untracked nested repository, or a tracked file that became a
    // folder (git lists the folder's files on their own).
    if meta.is_dir() {
        return Entry::Skipped;
    }

    // A FIFO, socket or device where git tracks a file: opening one can block.
    if !meta.is_file() {
        return Entry::Unreadable("is not a regular file".into());
    }

    if rel.is_note() {
        return match std::fs::read(&abs).map(String::from_utf8) {
            Ok(Ok(text)) => Entry::Note(text),
            Ok(Err(_)) => Entry::Unreadable("is not valid UTF-8".into()),
            Err(e) => Entry::Unreadable(format!("cannot be read: {e}")),
        };
    }

    // Core never needs the bytes of a source or an asset, so opening it is the check.
    // One loci cannot open, git cannot hash either: `git status` would report it
    // modified, and lint would say a source changed (`sources.changed`) that nobody
    // edited (D154).
    match std::fs::File::open(&abs) {
        Ok(_) => Entry::File,
        Err(e) => Entry::Unreadable(format!("cannot be read: {e}")),
    }
}

/// A refusal message listing `items`, one per line under `header`.
fn bulleted(header: &str, items: &[String]) -> String {
    format!("{header}:\n  {}", items.join("\n  "))
}

/// Writes a file via a temporary sibling and a rename, so readers never see a partial file.
/// A file it replaces keeps its permissions.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    persist(stage(path, bytes)?, path)
}

/// `bytes` in a uniquely named file in `path`'s folder, synced, not yet in its place
/// ([`persist`]). On Unix it is created with mode 0o666 less the umask, as a plain
/// write creates a file (tempfile's default is owner-only).
pub(crate) fn stage(path: &Path, bytes: &[u8]) -> std::io::Result<tempfile::NamedTempFile> {
    use std::io::Write;
    let dir = path.parent().expect("an absolute file path has a folder");
    let builder = &mut tempfile::Builder::new();
    #[cfg(unix)]
    builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o666));
    let mut tmp = builder.tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    Ok(tmp)
}

/// Puts a staged file in place of `path`; a file it replaces keeps its permissions.
pub(crate) fn persist(tmp: tempfile::NamedTempFile, path: &Path) -> std::io::Result<()> {
    if let Ok(meta) = std::fs::metadata(path) {
        tmp.as_file().set_permissions(meta.permissions())?;
    }
    tmp.persist(path).map(drop).map_err(|e| e.error)
}

/// Copies the directory `from` to `to`, recursively, leaving out the entries of `from`
/// itself named in `skip`. A link to a file is copied as the file; any other link
/// fails, since a copied link could lead back into the vault. An error names the path
/// it was about.
pub fn copy_dir(from: &Path, to: &Path, skip: &[&str]) -> std::io::Result<()> {
    std::fs::create_dir_all(to).map_err(at(to))?;
    for entry in std::fs::read_dir(from).map_err(at(from))? {
        let entry = entry.map_err(at(from))?;
        if skip.iter().any(|s| entry.file_name() == *s) {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type().map_err(at(&src))?;
        if kind.is_dir() {
            copy_dir(&src, &dst, &[])?
        } else {
            // A link is no dir here (`file_type` does not follow it); `fs::copy` follows
            // it and refuses anything but a regular file.
            std::fs::copy(&src, &dst).map(drop).map_err(at(&src))?
        }
    }
    Ok(())
}

/// An io error with the path it was about, for `map_err`.
fn at(path: &Path) -> impl Fn(std::io::Error) -> std::io::Error + '_ {
    move |e| std::io::Error::new(e.kind(), format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::vp;

    #[cfg(unix)]
    #[test]
    fn copy_dir_names_a_link_it_cannot_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let (from, to) = (tmp.path().join("from"), tmp.path().join("to"));
        std::fs::create_dir_all(from.join("d")).unwrap();
        std::os::unix::fs::symlink("d", from.join("dir-link")).unwrap();
        let e = copy_dir(&from, &to, &[]).unwrap_err();
        assert!(e.to_string().contains("dir-link"), "{e}");
    }

    #[test]
    fn written_paths_resolve_exactly_then_with_md() {
        let v =
            crate::testing::vault(&[("a.md", ""), ("a.md.md", ""), ("x.png", ""), ("x.png.md", ""), ("n/b.md", "")]);
        let resolve = |w: &str| v.resolve(w).map(VaultPath::as_str);
        assert_eq!(resolve("n/b"), Some("n/b.md"), "a note without `.md`");
        assert_eq!(resolve("a"), Some("a.md"));
        assert_eq!(resolve("a.md"), Some("a.md"), "an exact path first");
        assert_eq!(resolve("a.md.md"), Some("a.md.md"));
        assert_eq!(resolve("x.png"), Some("x.png"));
        assert_eq!(resolve("x.png.md"), Some("x.png.md"));
        assert_eq!(resolve("nope"), None);
        // Each written form resolves back to its file.
        for f in v.files() {
            assert_eq!(v.resolve(&crate::relink::written(&v, f)), Some(f), "{f}");
        }
        let written = |p: &str| crate::relink::written(&v, &vp(p));
        assert_eq!(
            (written("n/b.md"), written("a.md.md"), written("x.png.md")),
            ("n/b".into(), "a.md.md".into(), "x.png.md".into())
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_rewritten_file_keeps_its_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.md");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o664)).unwrap();
        write_atomic(&path, b"new").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!((std::fs::read_to_string(&path).unwrap(), mode), ("new".into(), 0o664));
    }

    #[cfg(unix)]
    #[test]
    fn a_new_file_gets_the_mode_of_a_plain_write() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let (plain, staged) = (dir.path().join("plain"), dir.path().join("staged"));
        std::fs::write(&plain, "x").unwrap();
        write_atomic(&staged, b"x").unwrap();
        assert_eq!(mode(&staged), mode(&plain));
    }
}
