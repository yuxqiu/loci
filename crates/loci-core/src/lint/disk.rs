//! The rules that compare the working tree with HEAD (m0 §7.3): they read git, not only the
//! index.

use std::path::Path;

use crate::envelope::Diagnostic;
use crate::git;
use crate::index::Index;
use crate::path::VaultPath;

/// The codes of [`disk_rules`].
pub(super) const DISK_CODES: &[&str] = &["sources.changed", "material-dropped", "git.failed"];

/// The rules that compare the working tree with HEAD (§7.3): `sources.changed` and
/// `material-dropped`. Without a HEAD there is nothing to compare; a git failure is
/// warning `git.failed`, and neither rule is checked. Unsorted; the caller merges them
/// with [`run`]'s.
pub fn disk_rules(index: &Index, root: &Path) -> Vec<Diagnostic> {
    let mut out = vec![];
    let changes = match git::has_head(root).and_then(|head| head.then(|| git::status(root)).transpose()) {
        Ok(Some(changes)) => changes,
        Ok(None) => return out,
        Err(e) => {
            let msg = format!("git failed, `sources.changed` and `material-dropped` were not checked: {e}");
            out.push(Diagnostic::warning("git.failed", msg));
            return out;
        }
    };
    sources_changed(&changes, &mut out);
    if let Err(e) = check_material(index, root, &changes, &mut out) {
        out.push(Diagnostic::warning("git.failed", format!("git failed, `material-dropped` was not checked: {e}")));
    }
    out
}

/// `sources.changed` (§6.4): a source's content differs from HEAD at the same path.
fn sources_changed(changes: &git::Changes, out: &mut Vec<Diagnostic>) {
    for p in changes.modified.iter().filter(|p| p.is_source_file()) {
        out.push(
            Diagnostic::error("sources.changed", format!("source `{p}` differs from its committed version"))
                .at_path(p.as_str())
                .with_hint("sources never change; add a new source instead"),
        );
    }
}

/// Only vault files count (m0 §3): a deleted path that core would read, and a current
/// file the index lists, whose counts the index already holds. Only reading `HEAD` can
/// fail.
fn check_material(index: &Index, root: &Path, changes: &git::Changes, out: &mut Vec<Diagnostic>) -> Result<(), String> {
    use crate::parse::Counts;

    // A `HEAD` blob that is not UTF-8 holds no parsed material.
    let head = |p: &VaultPath| {
        git::show_head(root, p).map(|b| match String::from_utf8(b) {
            Ok(text) => crate::parse::parse(p, p.kind().expect("a vault path"), text).counts.values(),
            Err(_) => [0; 3],
        })
    };

    let mut deleted = [0usize; 3];
    let mut added = [0usize; 3];

    // Per category, the first deleted path that held any (§7.1: vault-level rules
    // report on the first involved path).
    let mut first: [Option<&str>; 3] = [None; 3];
    let mut deleted_paths: Vec<&VaultPath> = changes.deleted.iter().filter(|p| p.is_note() && p.is_core()).collect();
    deleted_paths.sort();
    for p in deleted_paths {
        for (i, n) in head(p)?.into_iter().enumerate() {
            deleted[i] += n;
            if n > 0 && first[i].is_none() {
                first[i] = Some(p.as_str());
            }
        }
    }

    let new_files = changes.added.iter().chain(&changes.untracked).map(|p| (p, false));
    let changed = changes.modified.iter().map(|p| (p, true)).chain(new_files);
    let listed = |p: &VaultPath| index.contains(p);
    for (p, in_head) in changed.filter(|(p, _)| p.is_note() && listed(p)) {
        let now = index.parsed(p).expect("a listed `.md` file is parsed").counts.values();
        let before = if in_head { head(p)? } else { [0; 3] };
        for i in 0..3 {
            added[i] += now[i].saturating_sub(before[i]);
        }
    }

    for (i, name) in Counts::NAMES.iter().enumerate() {
        if deleted[i] > added[i] {
            let path = first[i].expect("deleted[i] > 0 implies a deleted file held some");
            out.push(
                Diagnostic::warning(
                    "material-dropped",
                    format!("deleted files held {} {name} but only {} were added elsewhere", deleted[i], added[i]),
                )
                .at_path(path),
            );
        }
    }

    Ok(())
}
