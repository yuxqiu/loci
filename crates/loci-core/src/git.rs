//! Git helpers. loci calls the `git` CLI.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::path::VaultPath;

/// A git work tree: git commands run from its root.
#[derive(Debug, Clone, Copy)]
pub struct Repo<'a>(&'a Path);

impl<'a> Repo<'a> {
    pub fn new(root: &'a Path) -> Self {
        Repo(root)
    }

    fn git(&self) -> Command {
        let mut c = Command::new("git");
        // No advice (hints for people, such as a sparse index expanding) and no trace
        // (the `GIT_TRACE*` variables, and trace2 targets, which config can set and
        // these variables override): what git prints on stderr is then an error or a
        // warning loci reports.
        c.arg("-C").arg(self.0).env("GIT_ADVICE", "0");
        for (var, _) in std::env::vars_os().filter(|(v, _)| v.as_encoded_bytes().starts_with(b"GIT_TRACE")) {
            c.env_remove(var);
        }
        c.env("GIT_TRACE2", "0").env("GIT_TRACE2_PERF", "0").env("GIT_TRACE2_EVENT", "0");
        c
    }

    /// Runs git and returns stdout, or an error message including stderr.
    fn run(&self, args: &[&str]) -> Result<Vec<u8>, String> {
        stdout(self.git().args(args), &args.join(" "))
    }

    /// Creates the repository (`git init`).
    pub fn init(&self) -> Result<(), String> {
        self.run(&["init", "-q"]).map(drop)
    }

    /// Marks `path` intent-to-add, so `git diff` shows a new file.
    pub fn intent_to_add(&self, path: &VaultPath) -> Result<(), String> {
        self.run(&["add", "-N", "--", path.as_str()]).map(drop)
    }

    /// Commits `paths` alone with `message`, as the user; returns the commit. When adding
    /// or committing fails (an ignored path, a hook refusing the commit), `paths` are
    /// unstaged again: git adds what it may before refusing.
    pub fn commit_paths(&self, message: &str, paths: &[VaultPath]) -> Result<String, String> {
        let paths: Vec<&str> = paths.iter().map(VaultPath::as_str).collect();
        let paths = paths.as_slice();
        let committed = self
            .run(&[&["add", "--"], paths].concat())
            .and_then(|_| self.run(&[&["commit", "-q", "-m", message, "--"], paths].concat()));
        if let Err(e) = committed {
            return Err(match self.run(&[&["reset", "-q", "--"], paths].concat()) {
                Ok(_) => e,
                Err(r) => format!("{e}; unstaging the paths also failed: {r}"),
            });
        }
        self.head()
    }

    /// Makes `to/.git` a repository of its own with this one's history, HEAD and index,
    /// so that commits in a copy of the work tree at `to` stay there. It copies the
    /// repository's folder (in a linked worktree or a submodule, `.git` is only a file
    /// naming it, shared with the copy), then a linked worktree's own folder (HEAD,
    /// index, split index) over it, without `NOT_COPIED`; the copy's work tree is `to`.
    pub fn copy_to(&self, to: &Path) -> Result<(), String> {
        let git_path = |arg: &str| -> Result<PathBuf, String> {
            path_line(self.run(&["rev-parse", "--path-format=absolute", arg])?, arg).map(PathBuf::from)
        };
        let (common, own) = (git_path("--git-common-dir")?, git_path("--git-dir")?);
        let dot_git = to.join(".git");
        for dir in if own == common { vec![common] } else { vec![common, own] } {
            crate::vault::copy_dir(&dir, &dot_git, &NOT_COPIED).map_err(|e| e.to_string())?;
        }
        // A submodule's config names its work tree. The file by name, so no repository
        // lookup (`GIT_DIR`) can redirect the write.
        let mut set = Command::new("git");
        set.arg("config").arg("--file").arg(dot_git.join("config")).arg("core.worktree").arg(to);
        stdout(&mut set, "config core.worktree").map(drop)
    }

    /// The id of the commit `HEAD` names.
    pub fn head(&self) -> Result<String, String> {
        self.run(&["rev-parse", "HEAD"]).map(trimmed)
    }

    /// The agent's notes in the record `name`, or none if it has no notes or does not
    /// exist.
    fn agent_notes(&self, name: &str) -> Result<Vec<String>, String> {
        if !self.answer(&["rev-parse", "--verify", "-q", name])? {
            return Ok(vec![]);
        }
        let message = String::from_utf8_lossy(&self.run(&["log", "-1", "--format=%B", name])?).into_owned();
        Ok(message.lines().filter_map(|l| l.strip_prefix("- ")).map(str::to_string).collect())
    }

    /// Records the working tree as a commit whose parent is HEAD, under
    /// `refs/loci/agent/<HEAD id>` (replacing an earlier record for the same HEAD),
    /// without touching the index or any branch. `now` dates the record.
    /// `notes` (what was hard in the task) join those of the record it replaces. Returns
    /// the ref, or `None` when the tree equals HEAD's and there are no notes. Without
    /// HEAD there is nothing to record against, so the work would be lost: an error.
    pub fn record_agent_tree(&self, now: SystemTime, notes: &[String]) -> Result<Option<String>, String> {
        if !self.has_head()? {
            return Err("the repository has no commit yet; commit once so the work can be recorded".into());
        }

        let head = self.head()?;
        let tree = self.working_tree(&head)?;
        let name = format!("{AGENT_REFS}{head}");
        let all = merged_notes(self.agent_notes(&name)?, notes);

        if all.is_empty() && tree == trimmed(self.run(&["rev-parse", &format!("{head}^{{tree}}")])?) {
            return Ok(None);
        }

        let mut message = RECORD_SUBJECT.to_string();
        if !all.is_empty() {
            message.push_str("\n\n");
            message.push_str(&all.iter().map(|n| format!("- {n}\n")).collect::<String>());
        }

        let date = format!("@{} +0000", unix_seconds(now));
        let commit = stdout(
            self.git()
                .env("GIT_AUTHOR_NAME", "loci")
                .env("GIT_AUTHOR_EMAIL", "loci@localhost")
                .env("GIT_AUTHOR_DATE", &date)
                .env("GIT_COMMITTER_NAME", "loci")
                .env("GIT_COMMITTER_EMAIL", "loci@localhost")
                .env("GIT_COMMITTER_DATE", &date)
                .args(["commit-tree", &tree, "-p", &head, "--no-gpg-sign", "-m", &message]),
            "commit-tree",
        )?;
        self.run(&["update-ref", &name, &trimmed(commit)])?;
        Ok(Some(name))
    }

    /// The id of a git tree holding the working tree as `git add -A` would stage it, built
    /// in a private index from `head`, so the user's staging is untouched.
    fn working_tree(&self, head: &str) -> Result<String, String> {
        let git_dir = path_line(self.run(&["rev-parse", "--absolute-git-dir"])?, "the git directory's path")?;
        // Removed when `scratch` drops, on any return.
        let scratch = tempfile::Builder::new()
            .prefix("loci-agent-")
            .tempdir_in(git_dir)
            .map_err(|e| format!("cannot create a temporary index: {e}"))?;
        let index = scratch.path().join("index");
        let with_index = |args: &[&str]| stdout(self.git().env("GIT_INDEX_FILE", &index).args(args), &args.join(" "));
        let tree = with_index(&["read-tree", head])
            .and_then(|_| with_index(&["add", "-A"]))
            .and_then(|_| with_index(&["write-tree"]).map(trimmed))?;
        scratch.close().map_err(|e| format!("cannot remove the temporary index: {e}"))?;
        Ok(tree)
    }

    /// The agent records (`loci done`), as (ref, time of the record), by ref name.
    pub fn agent_refs(&self) -> Result<Vec<(String, SystemTime)>, String> {
        let out = self.run(&["for-each-ref", "--format=%(refname) %(committerdate:unix)", AGENT_REFS])?;
        let text = String::from_utf8_lossy(&out);
        text.lines()
            .map(|l| {
                let (name, time) = l.rsplit_once(' ').ok_or_else(|| format!("unexpected for-each-ref line `{l}`"))?;
                let time: u64 = time.parse().map_err(|_| format!("unexpected for-each-ref line `{l}`"))?;
                Ok((name.to_string(), UNIX_EPOCH + Duration::from_secs(time)))
            })
            .collect()
    }

    /// Deletes agent records older than `max_age` at time `now`; returns how many.
    pub fn prune_agent_refs(&self, now: SystemTime, max_age: Duration) -> Result<usize, String> {
        let old = |t: SystemTime| now.duration_since(t).is_ok_and(|age| age > max_age);
        let old: Vec<String> = self.agent_refs()?.into_iter().filter(|(_, t)| old(*t)).map(|(n, _)| n).collect();
        for name in &old {
            self.run(&["update-ref", "-d", name])?;
        }
        Ok(old.len())
    }

    /// Whether `path` is gitignored (`git check-ignore`: exit 0 ignored, 1 not ignored,
    /// anything else an error).
    pub fn is_ignored(&self, path: &VaultPath) -> Result<bool, String> {
        self.answer(&["check-ignore", "-q", "--", path.as_str()])
    }

    /// What git names, for opening the vault: the paths it sees in the work tree
    /// (tracked files and untracked files it does not ignore, `git ls-files -z --cached
    /// --others --exclude-standard`), and every name that is no vault path among them or
    /// in HEAD's tree (a deletion may be only there). Together these are every name git
    /// reports without `--ignored` (m0 §3); ignored files loci never names. Without
    /// fsmonitor, whose hook's stderr git passes on even when listing succeeds.
    pub(crate) fn ls_files(&self) -> Result<LsFiles, String> {
        let out = checked(self.git().args(["-c", "core.fsmonitor=false"]).args(LS_FILES), "ls-files")?;
        let (paths, mut name_problems) = names(nul_separated(&out.stdout));
        let (_, head_problems) = names(nul_separated(&self.head_names()?));
        name_problems.extend(head_problems);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let warnings = stderr.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect();
        Ok(LsFiles { paths, name_problems, warnings })
    }

    /// The paths git sees under `folder`: tracked files and untracked files it does not
    /// ignore.
    pub fn ls_folder(&self, folder: &VaultPath) -> Result<Vec<VaultPath>, String> {
        // A vault path, never a pattern: `*` or `?` in a folder name is literal.
        let pathspec = format!(":(literal){folder}");
        vault_paths(nul_separated(&self.run(&[&LS_FILES[..], &["--", &pathspec]].concat())?))
    }

    /// The names in HEAD's tree, NUL-separated; none without a commit.
    fn head_names(&self) -> Result<Vec<u8>, String> {
        if !self.has_head()? {
            return Ok(vec![]);
        }
        self.run(&["ls-tree", "-r", "-z", "--name-only", "HEAD"])
    }

    /// Whether the repository has a commit.
    pub fn has_head(&self) -> Result<bool, String> {
        self.answer(&["rev-parse", "--verify", "-q", "HEAD"])
    }

    /// Runs a git query that exits 0 for yes and 1 for no; any other exit is an error.
    fn answer(&self, args: &[&str]) -> Result<bool, String> {
        let what = args.join(" ");
        let out = output(self.git().args(args), &what)?;
        match out.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(failure(&what, &out)),
        }
    }

    /// The content of `path` at HEAD; an error when HEAD has no such file.
    pub fn show_head(&self, path: &VaultPath) -> Result<Vec<u8>, String> {
        self.run(&["cat-file", "blob", &format!("HEAD:{path}")])
    }

    /// The working tree's changes relative to HEAD (`git -c core.fileMode=false status
    /// --porcelain=v2 -z --untracked-files=all --no-renames --ignore-submodules=all`): a
    /// rename is a deletion and an addition, a permission-only change is no change, and
    /// submodules, whose content loci does not read, are left out.
    /// A changed path is filed by whether it is in HEAD and on disk, both read directly: a
    /// record's own codes misstate HEAD for intent-to-add files and for conflicts whose
    /// "ours" is not HEAD (`git stash apply`), and its worktree mode is copied from the
    /// index for skip-worktree and assume-unchanged files.
    pub fn status(&self) -> Result<Changes, String> {
        let out = self.run(&[
            "-c",
            "core.fileMode=false",
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--no-renames",
            "--ignore-submodules=all",
        ])?;
        let (changed, untracked) = parse_status(&out)?;
        let head_files: BTreeSet<VaultPath> = vault_paths(nul_separated(&self.head_names()?))?.into_iter().collect();

        let mut ch = Changes { untracked: untracked.into_iter().collect(), ..Changes::default() };
        for path in changed {
            let set = match (head_files.contains(&path), crate::vault::is_regular_file(self.0, &path)) {
                (true, true) => &mut ch.modified,
                (false, true) => &mut ch.added,
                (true, false) => &mut ch.deleted,
                (false, false) => continue,
            };
            set.insert(path);
        }

        Ok(ch)
    }
}

/// Runs a prepared git command without stdin. Failing to start it is an error naming
/// `what`.
fn output(cmd: &mut Command, what: &str) -> Result<Output, String> {
    cmd.stdin(Stdio::null()).output().map_err(|e| format!("git {what}: {e}"))
}

/// The environment variables that point git at a repository other than the one it
/// would find (`git rev-parse --local-env-vars`: `GIT_DIR`, `GIT_INDEX_FILE`, …).
pub fn location_vars() -> Result<Vec<String>, String> {
    let out = stdout(Command::new("git").args(["rev-parse", "--local-env-vars"]), "rev-parse --local-env-vars")?;
    Ok(String::from_utf8_lossy(&out).lines().map(str::to_string).collect())
}

/// The error for a git command that ran and failed: `what` and git's stderr.
fn failure(what: &str, out: &Output) -> String {
    format!("git {what}: {}", String::from_utf8_lossy(&out.stderr).trim())
}

/// Runs a prepared git command and returns its output, or an error naming `what` when
/// it fails.
fn checked(cmd: &mut Command, what: &str) -> Result<Output, String> {
    let out = output(cmd, what)?;
    if out.status.success() { Ok(out) } else { Err(failure(what, &out)) }
}

/// Runs a prepared git command and returns stdout, or an error naming `what`.
fn stdout(cmd: &mut Command, what: &str) -> Result<Vec<u8>, String> {
    checked(cmd, what).map(|out| out.stdout)
}

/// Prefix of the local refs that record an agent's finished work (`loci done`).
/// Refs outside `refs/heads/` and `refs/tags/` are not pushed by default.
pub(crate) const AGENT_REFS: &str = "refs/loci/agent/";

/// Whole seconds since the Unix epoch, as git dates take them.
fn unix_seconds(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).expect("the clock is after 1970").as_secs()
}

fn trimmed(out: Vec<u8>) -> String {
    String::from_utf8_lossy(&out).trim().to_string()
}

/// What [`Repo::copy_to`] leaves out of a git folder: the linked worktrees, the files
/// that link a worktree's own folder back to its repository, and the socket git's
/// fsmonitor daemon listens on (a socket cannot be copied).
const NOT_COPIED: [&str; 5] = ["worktrees", "commondir", "gitdir", "locked", "fsmonitor--daemon.ipc"];

/// Every path git sees in the work tree: tracked, and untracked but not ignored.
const LS_FILES: [&str; 5] = ["ls-files", "-z", "--cached", "--others", "--exclude-standard"];

/// The first line of a record's commit message; the agent's notes follow, one
/// `- <note>` line each.
const RECORD_SUBJECT: &str = "loci done: the agent's work";

/// `previous` notes followed by the new `notes` not among them, each with its
/// whitespace collapsed; blank notes are dropped.
fn merged_notes(previous: Vec<String>, notes: &[String]) -> Vec<String> {
    let mut all = previous;
    for n in notes.iter().map(|n| n.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|n| !n.is_empty()) {
        if !all.contains(&n) {
            all.push(n);
        }
    }
    all
}

/// What git names, for opening the vault ([`Repo::ls_files`]).
#[derive(Debug)]
pub(crate) struct LsFiles {
    /// The listed names that are vault paths. An untracked nested repository is listed
    /// as its folder.
    pub paths: Vec<VaultPath>,
    /// Why each other listed or HEAD name is no vault path (not UTF-8, or a control
    /// character), for messages; loci opens no vault that has one.
    pub name_problems: Vec<String>,
    /// The warnings git printed, one per line (a directory it could not open).
    pub warnings: Vec<String>,
}

/// The NUL-separated records of `out`.
fn nul_separated(out: &[u8]) -> impl Iterator<Item = &[u8]> {
    out.split(|b| *b == 0).filter(|r| !r.is_empty())
}

/// The names git printed: those that are vault paths, and why each other one is not.
fn names<'a>(out: impl IntoIterator<Item = &'a [u8]>) -> (Vec<VaultPath>, Vec<String>) {
    let (mut paths, mut bad) = (vec![], vec![]);
    for name in out {
        let path = std::str::from_utf8(name)
            .map_err(|_| format!("\"{}\" has a name that is not UTF-8", name.escape_ascii()))
            .and_then(vault_path);
        match path {
            Ok(path) => paths.push(path),
            Err(e) => bad.push(e),
        }
    }
    (paths, bad)
}

/// The names git printed, after the vault opened: a name that is no vault path means
/// the vault changed since (it refused one at open), an error.
fn vault_paths<'a>(out: impl IntoIterator<Item = &'a [u8]>) -> Result<Vec<VaultPath>, String> {
    match names(out) {
        (paths, bad) if bad.is_empty() => Ok(paths),
        (_, bad) => Err(format!("{} (the vault changed since loci opened it)", bad.join(", "))),
    }
}

/// The top level of the git work tree holding `dir`, canonicalized. The error is git's
/// message, for example when `dir` is in no work tree.
pub fn toplevel(dir: &Path) -> Result<PathBuf, String> {
    let top = path_line(Repo::new(dir).run(&["rev-parse", "--show-toplevel"])?, "the work tree's path")?;
    Path::new(&top).canonicalize().map_err(|e| format!("{top}: {e}"))
}

/// A path git printed on a line of its own, without the line break (and nothing else: a
/// folder name may end in a space). An error naming `what` when it is not UTF-8.
fn path_line(out: Vec<u8>, what: &str) -> Result<String, String> {
    let out =
        String::from_utf8(out).map_err(|e| format!("{what} is not UTF-8: \"{}\"", e.as_bytes().escape_ascii()))?;
    Ok(out.strip_suffix('\n').unwrap_or(&out).to_string())
}

/// A name git lists, as a vault path: an untracked nested repository's trailing `/`
/// dropped, so that it names the folder. Git lists relative paths without empty, `.` or
/// `..` segments, so the error, shown escaped, is a name with a control character.
fn vault_path(name: &str) -> Result<VaultPath, String> {
    let name = name.strip_suffix('/').unwrap_or(name);
    VaultPath::new(name.to_string()).ok_or_else(|| format!("{name:?} is not a vault path: it has a control character"))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    pub modified: BTreeSet<VaultPath>,
    pub deleted: BTreeSet<VaultPath>,
    pub added: BTreeSet<VaultPath>,
    pub untracked: BTreeSet<VaultPath>,
}

impl Changes {
    /// Whether nothing changed.
    pub fn is_clean(&self) -> bool {
        [&self.modified, &self.deleted, &self.added, &self.untracked].iter().all(|c| c.is_empty())
    }
}

/// The changed paths (ordinary and conflict records) and the untracked paths. A record
/// that does not have its fields is an error, not a path.
fn parse_status(out: &[u8]) -> Result<(Vec<VaultPath>, Vec<VaultPath>), String> {
    let (mut changed, mut untracked) = (vec![], vec![]);
    for rec in nul_separated(out) {
        let shown = || rec.escape_ascii();
        // 1 XY sub mH mI mW hH hI path; u XY sub m1 m2 m3 mW h1 h2 h3 path; ? path.
        let (fields, list) = match rec[0] {
            b'1' => (9, &mut changed),
            b'u' => (11, &mut changed),
            b'?' => (2, &mut untracked),
            // Headers: none requested, and git-status(1) says to ignore unknown ones.
            b'#' => continue,
            // Renamed (`2`) and ignored (`!`) records are not requested.
            _ => return Err(format!("git status: unexpected record `{}`", shown())),
        };
        let path = rec
            .splitn(fields, |b| *b == b' ')
            .nth(fields - 1)
            .ok_or_else(|| format!("git status: malformed record `{}`", shown()))?;
        list.push(path);
    }
    Ok((vault_paths(changed)?, vault_paths(untracked)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(set: &BTreeSet<VaultPath>) -> Vec<&str> {
        set.iter().map(VaultPath::as_str).collect()
    }

    #[test]
    fn status_records() {
        let out = [
            "1 .M N... 100644 100644 100644 e69de29 e69de29 m.md",
            "1 .D N... 100644 100644 000000 e69de29 e69de29 wd.md",
            "u UU N... 100644 100644 100644 100644 7898192 f2ad6c7 6178079 c d.md",
            "u DD N... 100644 000000 000000 000000 7898192 0000000 0000000 dd.md",
            "? u.md",
            "# branch.oid abc",
        ]
        .join("\0");
        let (changed, untracked) = parse_status(out.as_bytes()).unwrap();
        assert_eq!(changed, ["m.md", "wd.md", "c d.md", "dd.md"]);
        assert_eq!(untracked, ["u.md"]);
        assert!(parse_status(b"1 .M N... too short").is_err(), "a record without its fields is no path");
        assert!(parse_status(b"! ignored.md").is_err(), "an unrequested record is an error");
    }

    /// An empty repository whose settings override the user's own: no signing prompts,
    /// no hooks.
    fn test_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        test_repo_at(tmp.path());
        tmp
    }

    fn test_repo_at(dir: &Path) {
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "t@t"],
            &["config", "user.name", "t"],
            &["config", "commit.gpgsign", "false"],
            &["config", "core.hooksPath", "/dev/null"],
        ] {
            Repo::new(dir).run(args).unwrap();
        }
    }

    /// A submodule (a gitlink, a directory on disk) with changes is not reported.
    #[test]
    fn status_ignores_submodules() {
        let tmp = test_repo();
        let root = tmp.path();
        let git = |args: &[&str]| Repo::new(root).run(args).unwrap();
        std::fs::write(root.join("a.md"), "a\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "base"]);
        let base = trimmed(git(&["rev-parse", "HEAD"]));
        git(&["update-index", "--add", "--cacheinfo", &format!("160000,{base},sub")]);
        git(&["commit", "-qm", "sub"]);
        // A checked-out submodule at a different commit, with an uncommitted file.
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        test_repo_at(&sub);
        std::fs::write(sub.join("x.md"), "x\n").unwrap();
        Repo::new(&sub).run(&["add", "."]).unwrap();
        Repo::new(&sub).run(&["commit", "-qm", "x"]).unwrap();
        std::fs::write(sub.join("y.md"), "y\n").unwrap();
        assert_eq!(Repo::new(root).status().unwrap(), Changes::default());
    }

    /// A permission-only change is not a change to the content.
    #[cfg(unix)]
    #[test]
    fn status_ignores_file_modes() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = test_repo();
        let root = tmp.path();
        std::fs::write(root.join("a.md"), "a\n").unwrap();
        Repo::new(root).run(&["add", "."]).unwrap();
        Repo::new(root).run(&["commit", "-qm", "base"]).unwrap();
        std::fs::set_permissions(root.join("a.md"), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(Repo::new(root).status().unwrap(), Changes::default());
    }

    /// Cases where git's own records mislead: a stash conflict whose "ours" is not HEAD,
    /// a rename whose new file is gone, and a copy under `status.renames=copies`.
    #[test]
    fn status_against_git() {
        let tmp = test_repo();
        let root = tmp.path();
        let git = |args: &[&str]| Repo::new(root).run(args).unwrap();
        let write = |p: &str, t: &str| std::fs::write(root.join(p), t).unwrap();
        git(&["config", "status.renames", "copies"]);
        write("a.md", "a\n");
        write("r.md", "r\n");
        git(&["add", "."]);
        git(&["commit", "-qm", "base"]);

        git(&["mv", "r.md", "gone.md"]);
        std::fs::remove_file(root.join("gone.md")).unwrap();

        // Intent-to-add, then deleted: no change. Re-added as intent-to-add after
        // `rm --cached` (`DA`): modified.
        write("n.md", "n\n");
        git(&["add", "-N", "n.md"]);
        std::fs::remove_file(root.join("n.md")).unwrap();
        git(&["rm", "-q", "--cached", "a.md"]);
        git(&["add", "-N", "a.md"]);
        let ch = Repo::new(root).status().unwrap();
        assert_eq!(strs(&ch.modified), ["a.md"]);
        assert_eq!(strs(&ch.deleted), ["r.md"]);
        assert_eq!(strs(&ch.added), Vec::<&str>::new());
        git(&["add", "a.md"]);
        git(&["rm", "-q", "--cached", "n.md"]);

        // A staged change to a skip-worktree file that is gone from disk: deleted.
        write("a.md", "a2\n");
        git(&["add", "a.md"]);
        git(&["update-index", "--skip-worktree", "a.md"]);
        std::fs::remove_file(root.join("a.md")).unwrap();
        assert_eq!(strs(&Repo::new(root).status().unwrap().deleted), ["a.md", "r.md"]);
        git(&["update-index", "--no-skip-worktree", "a.md"]);
        git(&["checkout", "-q", "HEAD", "--", "a.md"]);

        std::fs::copy(root.join("a.md"), root.join("copy.md")).unwrap();
        git(&["add", "copy.md"]);
        write("y.md", "one\n");
        git(&["add", "y.md"]);
        git(&["stash", "-q"]);
        write("y.md", "two\n");
        git(&["add", "y.md"]);
        assert!(Repo::new(root).run(&["stash", "apply", "-q"]).is_err(), "the stash conflicts on y.md");

        // The stash also restores `gone.md`.
        let ch = Repo::new(root).status().unwrap();
        assert_eq!(strs(&ch.modified), Vec::<&str>::new());
        assert_eq!(strs(&ch.deleted), ["r.md"]);
        assert_eq!(strs(&ch.added), ["copy.md", "gone.md", "y.md"]);
    }

    /// A changed name that is no vault path (a control character) is an error, not a
    /// panic: a file whose deletion is staged is no longer listed, so opening the vault
    /// does not refuse it.
    #[test]
    fn status_refuses_a_name_that_is_no_vault_path() {
        let tmp = test_repo();
        let root = tmp.path();
        let git = |args: &[&str]| Repo::new(root).run(args).unwrap();
        std::fs::write(root.join("a\nb.md"), "a\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "base"]);
        std::fs::remove_file(root.join("a\nb.md")).unwrap();
        let err = Repo::new(root).status().unwrap_err();
        assert!(err.contains(r#""a\nb.md" is not a vault path"#), "{err}");
    }

    /// A changed name that is not UTF-8 is an error too, not a change left uncounted.
    #[cfg(unix)]
    #[test]
    fn status_refuses_a_name_that_is_not_utf8() {
        use std::os::unix::ffi::OsStrExt;
        let tmp = test_repo();
        let root = tmp.path();
        let name = std::ffi::OsStr::from_bytes(b"n\xe9.md");
        std::fs::write(root.join(name), "n\n").unwrap();
        Repo::new(root).run(&["add", "."]).unwrap();
        Repo::new(root).run(&["commit", "-qm", "base"]).unwrap();
        std::fs::remove_file(root.join(name)).unwrap();
        let err = Repo::new(root).status().unwrap_err();
        assert!(err.contains(r#""n\xe9.md" has a name that is not UTF-8"#), "{err}");
    }

    /// A folder name may end in a space: only git's line break is dropped.
    #[test]
    fn toplevel_keeps_trailing_spaces() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("sp ");
        std::fs::create_dir(&dir).unwrap();
        test_repo_at(&dir);
        assert_eq!(toplevel(&dir).unwrap(), dir.canonicalize().unwrap());
    }
}
