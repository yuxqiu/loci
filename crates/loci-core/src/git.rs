//! Git helpers. loci calls the `git` CLI.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn git(root: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C").arg(root);
    c
}

/// Runs a prepared git command without stdin. Failing to start it is an error naming
/// `what`.
fn spawn(cmd: &mut Command, what: &str) -> Result<Output, String> {
    cmd.stdin(Stdio::null()).output().map_err(|e| format!("git {what}: {e}"))
}

/// The error for a git command that ran and failed: `what` and git's stderr.
fn failure(what: &str, out: &Output) -> String {
    format!("git {what}: {}", String::from_utf8_lossy(&out.stderr).trim())
}

/// Runs a prepared git command and returns stdout, or an error naming `what`.
fn exec(cmd: &mut Command, what: &str) -> Result<Vec<u8>, String> {
    let out = spawn(cmd, what)?;
    if out.status.success() { Ok(out.stdout) } else { Err(failure(what, &out)) }
}

/// Runs git and returns stdout, or an error message including stderr.
pub fn run(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    exec(git(root).args(args), &args.join(" "))
}

/// Prefix of the local refs that record an agent's finished work (`loci done`).
/// Refs outside `refs/heads/` and `refs/tags/` are not pushed by default.
pub const AGENT_REFS: &str = "refs/loci/agent/";

fn trimmed(out: Vec<u8>) -> String {
    String::from_utf8_lossy(&out).trim().to_string()
}

/// The id of the commit `HEAD` names.
pub fn head(root: &Path) -> Result<String, String> {
    run(root, &["rev-parse", "HEAD"]).map(trimmed)
}

/// The first line of a record's commit message; the agent's notes follow, one
/// `- <note>` line each.
const RECORD_SUBJECT: &str = "loci done: the agent's work";

/// The agent's notes in the record `name`, or none if it has no notes or does not
/// exist.
pub fn agent_notes(root: &Path, name: &str) -> Result<Vec<String>, String> {
    if !answer(root, &["rev-parse", "--verify", "-q", name])? {
        return Ok(vec![]);
    }
    let message = String::from_utf8_lossy(&run(root, &["log", "-1", "--format=%B", name])?).into_owned();
    Ok(message.lines().filter_map(|l| l.strip_prefix("- ")).map(str::to_string).collect())
}

/// Records the working tree as a commit whose parent is HEAD, under
/// `refs/loci/agent/<HEAD id>` (replacing an earlier record for the same HEAD),
/// without touching the index or any branch. `now` (Unix seconds) dates the record.
/// `notes` (what was hard in the task) join those of the record it replaces. Returns
/// the ref, or `None` when there is no HEAD, or when the tree equals HEAD's and there
/// are no notes.
pub fn record_agent_tree(root: &Path, now: u64, notes: &[String]) -> Result<Option<String>, String> {
    if !has_head(root)? {
        return Ok(None);
    }
    let head = head(root)?;
    let git_dir = trimmed(run(root, &["rev-parse", "--absolute-git-dir"])?);
    // A private index, so the user's staging is untouched.
    let index = Path::new(&git_dir).join(format!("loci-agent-index-{}", std::process::id()));
    let with_index = |args: &[&str]| exec(git(root).env("GIT_INDEX_FILE", &index).args(args), &args.join(" "));
    let tree = with_index(&["read-tree", &head])
        .and_then(|_| with_index(&["add", "-A"]))
        .and_then(|_| with_index(&["write-tree"]).map(trimmed));
    match std::fs::remove_file(&index) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
        _ => {}
    }
    let tree = tree?;
    let name = format!("{AGENT_REFS}{head}");
    let mut all = agent_notes(root, &name)?;
    for n in notes.iter().map(|n| n.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|n| !n.is_empty()) {
        if !all.contains(&n) {
            all.push(n);
        }
    }
    if all.is_empty() && tree == trimmed(run(root, &["rev-parse", &format!("{head}^{{tree}}")])?) {
        return Ok(None);
    }
    let mut message = RECORD_SUBJECT.to_string();
    if !all.is_empty() {
        message.push_str("\n\n");
        message.push_str(&all.iter().map(|n| format!("- {n}\n")).collect::<String>());
    }
    let date = format!("@{now} +0000");
    let commit = exec(
        git(root)
            .env("GIT_AUTHOR_NAME", "loci")
            .env("GIT_AUTHOR_EMAIL", "loci@localhost")
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_NAME", "loci")
            .env("GIT_COMMITTER_EMAIL", "loci@localhost")
            .env("GIT_COMMITTER_DATE", &date)
            .args(["commit-tree", &tree, "-p", &head, "--no-gpg-sign", "-m", &message]),
        "commit-tree",
    )?;
    run(root, &["update-ref", &name, &trimmed(commit)])?;
    Ok(Some(name))
}

/// The agent records (`loci done`), as (ref, Unix seconds of the record), by ref name.
pub fn agent_refs(root: &Path) -> Result<Vec<(String, u64)>, String> {
    let out = run(root, &["for-each-ref", "--format=%(refname) %(committerdate:unix)", AGENT_REFS])?;
    let text = String::from_utf8_lossy(&out);
    text.lines()
        .map(|l| {
            let (name, time) = l.rsplit_once(' ').ok_or_else(|| format!("unexpected for-each-ref line `{l}`"))?;
            let time = time.parse().map_err(|_| format!("unexpected for-each-ref line `{l}`"))?;
            Ok((name.to_string(), time))
        })
        .collect()
}

/// Deletes agent records older than `max_age` seconds at time `now`; returns how many.
pub fn prune_agent_refs(root: &Path, now: u64, max_age: u64) -> Result<usize, String> {
    let old: Vec<String> =
        agent_refs(root)?.into_iter().filter(|(_, t)| now.saturating_sub(*t) > max_age).map(|(n, _)| n).collect();
    for name in &old {
        run(root, &["update-ref", "-d", name])?;
    }
    Ok(old.len())
}

/// Whether `path` is gitignored (`git check-ignore`: exit 0 ignored, 1 not ignored,
/// anything else an error).
pub fn is_ignored(root: &Path, path: &str) -> Result<bool, String> {
    answer(root, &["check-ignore", "-q", "--", path])
}

/// Every path git sees in the work tree, or under `pathspec` when given: tracked files
/// and untracked files it does not ignore (`git ls-files -z --cached --others
/// --exclude-standard`), as raw names, and any warnings git printed (for example about
/// a directory it could not open).
pub fn ls_files(root: &Path, pathspec: Option<&str>) -> Result<(Vec<Vec<u8>>, String), String> {
    let what = "ls-files";
    let mut cmd = git(root);
    cmd.args(["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--"])
        // A vault path, never a pattern: `*` or `?` in a folder name is literal.
        .args(pathspec.map(|p| format!(":(literal){p}")));
    let out = spawn(&mut cmd, what)?;
    if !out.status.success() {
        return Err(failure(what, &out));
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let paths = out.stdout.split(|b| *b == 0).filter(|p| !p.is_empty()).map(<[u8]>::to_vec).collect();
    Ok((paths, stderr))
}

/// Whether the repository has a commit.
pub fn has_head(root: &Path) -> Result<bool, String> {
    answer(root, &["rev-parse", "--verify", "-q", "HEAD"])
}

/// Runs a git query that exits 0 for yes and 1 for no; any other exit is an error.
fn answer(root: &Path, args: &[&str]) -> Result<bool, String> {
    let what = args.join(" ");
    let out = spawn(git(root).args(args), &what)?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(failure(&what, &out)),
    }
}

/// The top level of the git work tree holding `dir`, canonicalized. The error is git's
/// message, for example when `dir` is in no work tree.
pub fn toplevel(dir: &Path) -> Result<PathBuf, String> {
    let top = trimmed(run(dir, &["rev-parse", "--show-toplevel"])?);
    Path::new(&top).canonicalize().map_err(|e| format!("{top}: {e}"))
}

/// The content of `<rev>:<path>`, if it exists.
pub fn show(root: &Path, rev: &str, path: &str) -> Result<Vec<u8>, String> {
    run(root, &["cat-file", "blob", &format!("{rev}:{path}")])
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
    pub added: Vec<String>,
    pub untracked: Vec<String>,
}

/// The working tree's changes relative to HEAD (`git -c core.fileMode=false status
/// --porcelain=v2 -z --untracked-files=all --no-renames --ignore-submodules=all`): a
/// rename is a deletion and an addition, a permission-only change is no change, and
/// submodules, whose content loci does not read, are left out.
/// A changed path is filed by whether it is in HEAD and on disk, both read directly: a
/// record's own codes misstate HEAD for intent-to-add files and for conflicts whose
/// "ours" is not HEAD (`git stash apply`), and its worktree mode is copied from the
/// index for skip-worktree and assume-unchanged files.
pub fn status(root: &Path) -> Result<Changes, String> {
    let out = run(
        root,
        &[
            "-c",
            "core.fileMode=false",
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--no-renames",
            "--ignore-submodules=all",
        ],
    )?;
    let (changed, untracked) = parse_status(&out);
    let head: BTreeSet<String> = if has_head(root)? {
        let out = run(root, &["ls-tree", "-r", "--name-only", "-z", "HEAD"])?;
        utf8_paths(&out).map(str::to_string).collect()
    } else {
        BTreeSet::new()
    };
    let mut ch = Changes { untracked, ..Changes::default() };
    for path in changed {
        match (head.contains(&path), crate::vault::is_regular_file(root, &path)) {
            (true, true) => ch.modified.push(path),
            (false, true) => ch.added.push(path),
            (true, false) => ch.deleted.push(path),
            (false, false) => {}
        }
    }
    Ok(ch)
}

/// The NUL-separated records of `out` that are valid UTF-8. A non-UTF-8 path is not a
/// vault file (under the folders core reads, the vault is refused as `file.unreadable`),
/// so it is skipped rather than mangled into a path that does not exist.
fn utf8_paths(out: &[u8]) -> impl Iterator<Item = &str> {
    out.split(|b| *b == 0).filter(|r| !r.is_empty()).filter_map(|r| std::str::from_utf8(r).ok())
}

/// The changed paths (ordinary and conflict records) and the untracked paths.
fn parse_status(out: &[u8]) -> (Vec<String>, Vec<String>) {
    let (mut changed, mut untracked) = (vec![], vec![]);
    for rec in utf8_paths(out) {
        // 1 XY sub mH mI mW hH hI path; u XY sub m1 m2 m3 mW h1 h2 h3 path.
        let fields = match rec.as_bytes().first() {
            Some(b'1') => 9,
            Some(b'u') => 11,
            Some(b'?') => {
                untracked.push(rec[2..].to_string());
                continue;
            }
            // Empty fields, headers (`#`) and ignored files (`!`): none requested.
            _ => continue,
        };
        changed.push(rec.splitn(fields, ' ').last().expect("a record has fields").to_string());
    }
    (changed, untracked)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let (changed, untracked) = parse_status(out.as_bytes());
        assert_eq!(changed, ["m.md", "wd.md", "c d.md", "dd.md"]);
        assert_eq!(untracked, ["u.md"]);
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
            run(dir, args).unwrap();
        }
    }

    /// A submodule (a gitlink, a directory on disk) with changes is not reported.
    #[test]
    fn status_ignores_submodules() {
        let tmp = test_repo();
        let root = tmp.path();
        let git = |args: &[&str]| run(root, args).unwrap();
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
        run(&sub, &["add", "."]).unwrap();
        run(&sub, &["commit", "-qm", "x"]).unwrap();
        std::fs::write(sub.join("y.md"), "y\n").unwrap();
        assert_eq!(status(root).unwrap(), Changes::default());
    }

    /// A permission-only change is not a change to the content.
    #[cfg(unix)]
    #[test]
    fn status_ignores_file_modes() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = test_repo();
        let root = tmp.path();
        std::fs::write(root.join("a.md"), "a\n").unwrap();
        run(root, &["add", "."]).unwrap();
        run(root, &["commit", "-qm", "base"]).unwrap();
        std::fs::set_permissions(root.join("a.md"), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(status(root).unwrap(), Changes::default());
    }

    /// Cases where git's own records mislead: a stash conflict whose "ours" is not HEAD,
    /// a rename whose new file is gone, and a copy under `status.renames=copies`.
    #[test]
    fn status_against_git() {
        let tmp = test_repo();
        let root = tmp.path();
        let git = |args: &[&str]| run(root, args).unwrap();
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
        let ch = status(root).unwrap();
        assert_eq!(ch.modified, ["a.md"]);
        assert_eq!((ch.deleted, ch.added), (vec!["r.md".to_string()], vec![]));
        git(&["add", "a.md"]);
        git(&["rm", "-q", "--cached", "n.md"]);

        // A staged change to a skip-worktree file that is gone from disk: deleted.
        write("a.md", "a2\n");
        git(&["add", "a.md"]);
        git(&["update-index", "--skip-worktree", "a.md"]);
        std::fs::remove_file(root.join("a.md")).unwrap();
        assert_eq!(status(root).unwrap().deleted, ["a.md", "r.md"]);
        git(&["update-index", "--no-skip-worktree", "a.md"]);
        git(&["checkout", "-q", "HEAD", "--", "a.md"]);

        std::fs::copy(root.join("a.md"), root.join("copy.md")).unwrap();
        git(&["add", "copy.md"]);
        write("y.md", "one\n");
        git(&["add", "y.md"]);
        git(&["stash", "-q"]);
        write("y.md", "two\n");
        git(&["add", "y.md"]);
        assert!(run(root, &["stash", "apply", "-q"]).is_err(), "the stash conflicts on y.md");

        // The stash also restores `gone.md`.
        let ch = status(root).unwrap();
        assert_eq!(ch.modified, Vec::<String>::new());
        assert_eq!(ch.deleted, ["r.md"]);
        assert_eq!(ch.added, ["copy.md", "gone.md", "y.md"]);
    }
}
