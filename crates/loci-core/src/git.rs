//! Git helpers. loci calls the `git` CLI.

use std::path::Path;
use std::process::{Command, Stdio};

fn git(root: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C").arg(root);
    c
}

/// Runs a prepared git command and returns stdout, or an error message naming `what`
/// and including stderr.
fn exec(cmd: &mut Command, what: &str) -> Result<Vec<u8>, String> {
    let out = cmd.stdin(Stdio::null()).output().map_err(|e| format!("git {what}: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!("git {what}: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
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

/// Records the working tree as a commit whose parent is HEAD, under
/// `refs/loci/agent/<HEAD id>` (replacing an earlier record for the same HEAD),
/// without touching the index or any branch. `now` (Unix seconds) dates the record.
/// Returns the ref, or `None` when there is no HEAD or the tree equals HEAD's.
pub fn record_agent_tree(root: &Path, now: u64) -> Result<Option<String>, String> {
    if !has_head(root) {
        return Ok(None);
    }
    let head = trimmed(run(root, &["rev-parse", "HEAD"])?);
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
    if tree == trimmed(run(root, &["rev-parse", &format!("{head}^{{tree}}")])?) {
        return Ok(None);
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
            .args(["commit-tree", &tree, "-p", &head, "--no-gpg-sign", "-m", "loci done: the agent's work"]),
        "commit-tree",
    )?;
    let name = format!("{AGENT_REFS}{head}");
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
    let out =
        git(root).args(["check-ignore", "-q", "--", path]).stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(format!("git check-ignore: {}", String::from_utf8_lossy(&out.stderr).trim())),
    }
}

/// True iff `dir` is the top level of a git work tree.
pub fn is_worktree_root(dir: &Path) -> bool {
    match run(dir, &["rev-parse", "--show-toplevel"]) {
        Ok(out) => {
            let top = String::from_utf8_lossy(&out).trim().to_string();
            let top = Path::new(&top).canonicalize().ok();
            top.is_some() && top == dir.canonicalize().ok()
        }
        Err(_) => false,
    }
}

/// True iff `dir` is inside some git work tree.
pub fn in_worktree(dir: &Path) -> bool {
    run(dir, &["rev-parse", "--is-inside-work-tree"]).is_ok()
}

pub fn has_head(root: &Path) -> bool {
    run(root, &["rev-parse", "--verify", "-q", "HEAD"]).is_ok()
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

/// The working tree's changes relative to HEAD (`git status --porcelain=v2 -z
/// --untracked-files=all`). Renames are reported as a deletion and an addition.
pub fn status(root: &Path) -> Result<Changes, String> {
    let out = run(root, &["status", "--porcelain=v2", "-z", "--untracked-files=all"])?;
    Ok(parse_status(&out))
}

fn parse_status(out: &[u8]) -> Changes {
    let mut ch = Changes::default();
    let mut fields = out.split(|b| *b == 0).map(|f| String::from_utf8_lossy(f).into_owned());
    while let Some(rec) = fields.next() {
        if rec.is_empty() {
            continue;
        }
        let kind = rec.as_bytes()[0];
        match kind {
            b'1' => {
                // 1 XY sub mH mI mW hH hI path
                let parts: Vec<&str> = rec.splitn(9, ' ').collect();
                let xy = parts[1];
                let path = parts[8].to_string();
                if xy.starts_with('A') || xy == ".A" {
                    // Not in HEAD; added then deleted (`AD`) is no change.
                    if !xy.ends_with('D') {
                        ch.added.push(path);
                    }
                } else if xy.contains('D') {
                    ch.deleted.push(path);
                } else {
                    ch.modified.push(path);
                }
            }
            b'2' => {
                // 2 XY sub mH mI mW hH hI Xscore path \0 origPath
                let parts: Vec<&str> = rec.splitn(10, ' ').collect();
                let path = parts[9].to_string();
                let orig = fields.next().expect("a rename record is followed by its original path");
                ch.deleted.push(orig);
                ch.added.push(path);
            }
            b'?' => ch.untracked.push(rec[2..].to_string()),
            _ => {}
        }
    }
    ch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_records() {
        let h =
            "100644 100644 100644 e69de29bb2d1d6434b8b29ae775ad8c2e48c5391 e69de29bb2d1d6434b8b29ae775ad8c2e48c5391";
        let out = [
            format!("1 .M N... {h} m.md"),
            format!("1 D. N... {h} d.md"),
            format!("1 .D N... {h} wd.md"),
            format!("1 A. N... {h} a.md"),
            format!("1 .A N... {h} n.md"),
            format!("1 AD N... {h} gone.md"),
            format!("2 R. N... {h} R100 new name.md"),
            "old name.md".to_string(),
            "? u.md".to_string(),
        ]
        .join("\0");
        let ch = parse_status(out.as_bytes());
        assert_eq!(ch.modified, ["m.md"]);
        assert_eq!(ch.deleted, ["d.md", "wd.md", "old name.md"]);
        assert_eq!(ch.added, ["a.md", "n.md", "new name.md"]);
        assert_eq!(ch.untracked, ["u.md"]);
    }
}
