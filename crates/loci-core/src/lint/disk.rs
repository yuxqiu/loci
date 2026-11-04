//! The rules that compare the working tree with HEAD (m0 §7.3): they read git, not only the
//! index.

use std::collections::BTreeMap;

use super::Code;
use crate::envelope::Diagnostic;
use crate::git;
use crate::index::Index;
use crate::parse::{self, Counts, Material};
use crate::path::{Place, VaultPath};

/// The codes of [`disk_rules`].
pub(super) const DISK_CODES: &[Code] = &[Code::SourcesChanged, Code::MaterialDropped, Code::GitFailed];

/// The rules that compare the working tree with HEAD (§7.3): `sources.changed` and
/// `material-dropped`. Without a HEAD there is nothing to compare. A git failure is
/// warning `git.failed`: when `git status` fails neither rule is checked; when reading
/// HEAD fails, `material-dropped` is not. Unsorted; the caller merges them with
/// [`super::run`]'s.
pub fn disk_rules(index: &Index, repo: git::Repo) -> Vec<Diagnostic> {
    let mut out = vec![];
    let changes = match changes_since_head(repo) {
        Ok(Some(changes)) => changes,
        Ok(None) => return out,
        Err(e) => {
            let msg = format!("git failed, `sources.changed` and `material-dropped` were not checked: {e}");
            out.push(Diagnostic::warning(Code::GitFailed, msg));
            return out;
        }
    };
    sources_changed(&changes, &mut out);
    if let Err(e) = check_material(index, repo, &changes, &mut out) {
        out.push(Diagnostic::warning(Code::GitFailed, format!("git failed, `material-dropped` was not checked: {e}")));
    }
    out
}

/// The working tree's changes against HEAD; `None` without a HEAD.
fn changes_since_head(repo: git::Repo) -> Result<Option<git::Changes>, String> {
    match repo.has_head()? {
        true => repo.status().map(Some),
        false => Ok(None),
    }
}

/// `sources.changed` (§6.4): a source's content differs from HEAD at the same path.
fn sources_changed(changes: &git::Changes, out: &mut Vec<Diagnostic>) {
    for p in changes.modified.iter().filter(|p| matches!(p.place(), Ok(Some(Place::Source { .. })))) {
        out.push(
            Diagnostic::error(Code::SourcesChanged, format!("source `{p}` differs from its committed version"))
                .at_path(p)
                .with_hint("sources never change; add a new source instead"),
        );
    }
}

/// What deleted files held of one material, and what was added elsewhere.
#[derive(Default)]
struct Tally<'a> {
    deleted: usize,
    added: usize,
    /// The first deleted path that held any (§7.1: vault-level rules report on the
    /// first involved path).
    first: Option<&'a VaultPath>,
}

/// `material-dropped`: for each material, a warning when deleted notes held more of it
/// than changed and new notes gained. Only vault files count (m0 §3): a deleted path that core would read, and a current
/// file the index lists, whose counts the index already holds. Only reading `HEAD` can
/// fail.
fn check_material(
    index: &Index,
    repo: git::Repo,
    changes: &git::Changes,
    out: &mut Vec<Diagnostic>,
) -> Result<(), String> {
    // A `HEAD` blob that is not UTF-8 holds no parsed material.
    let head = |p: &VaultPath| {
        repo.show_head(p).map(|b| String::from_utf8(b).map_or(Counts::default(), |t| parse::markdown(t).counts))
    };
    let mut tally: BTreeMap<Material, Tally> = BTreeMap::new();

    // Deleted: what the deleted notes held, in path order.
    let mut deleted: Vec<&VaultPath> = changes.deleted.iter().filter(|p| p.is_note() && p.is_core()).collect();
    deleted.sort();
    for p in deleted {
        let counts = head(p)?;
        for m in Material::ALL {
            let t = tally.entry(m).or_default();
            t.deleted += counts[m];
            if counts[m] > 0 && t.first.is_none() {
                t.first = Some(p);
            }
        }
    }

    // Added: what changed and new notes gained since HEAD.
    let new_files = changes.added.iter().chain(&changes.untracked).map(|p| (p, false));
    let changed = changes.modified.iter().map(|p| (p, true)).chain(new_files);
    for (p, note, in_head) in changed.filter_map(|(p, in_head)| Some((p, index.parsed(p)?, in_head))) {
        let now = note.counts;
        let before = if in_head { head(p)? } else { Counts::default() };
        for m in Material::ALL {
            tally.entry(m).or_default().added += now[m].saturating_sub(before[m]);
        }
    }

    for (m, t) in tally.iter().filter(|(_, t)| t.deleted > t.added) {
        let msg = format!("deleted files held {} {} but only {} were added elsewhere", t.deleted, m, t.added);
        let path = t.first.expect("a material deleted files held has a first such file");
        out.push(Diagnostic::warning(Code::MaterialDropped, msg).at_path(path));
    }
    Ok(())
}
