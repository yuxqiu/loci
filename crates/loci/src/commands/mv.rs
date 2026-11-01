//! `loci mv` (m0 §9.5).

use loci_core::envelope::{Outcome, Refusal, Report};
use loci_core::index::Index;
use std::os::unix::ffi::OsStrExt;

use loci_core::relink::Change;
use loci_core::vault::{self, Vault};
use loci_core::{git, mv, relink};
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, from: &str, to: &str) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let from = vault::path_arg(from)?;
    let to = vault::path_arg(to)?;
    let index = Index::open(&vault);
    let from = mv::stored_name(&index, &from)?;
    let exists = |p: &str| vault.abs(p).symlink_metadata().is_ok();
    let ignored = |p: &str| git::is_ignored(&vault.root, p).map_err(Refusal::git);
    let plan = mv::plan(&index, &from, &to, ignored, exists)?;
    // A folder moves whole: refuse to leave behind what loci does not move (a submodule).
    let left = if plan.moves.contains_key(&from) { vec![] } else { left_behind(&vault, &from, &plan)? };
    if !left.is_empty() {
        return Err(Refusal::refused(
            "mv.from",
            format!("`{from}` holds files loci does not move: {}; move them with git first", left.join(", ")),
        ));
    }
    // A rename must not hide an edit to a source from `sources.changed`.
    let changed = git::status(&vault.root).map_err(Refusal::git)?.modified;
    if let Some(p) = plan.moves.keys().find(|p| vault::is_source_file(p) && changed.contains(p)) {
        return Err(Refusal::refused(
            "mv.source-changed",
            format!("source `{p}` differs from its committed version; restore it before moving it"),
        ));
    }
    relink::apply(&vault.root, &plan).map_err(Refusal::io)?;
    let data = json!({
        "moved": plan.moves.iter().collect::<Vec<_>>(),
        "edited": plan.edit_counts.iter().map(|(p, n)| json!({"path": p, "edits": n})).collect::<Vec<_>>(),
    });
    let human = format!("moved {} file(s), edited {} file(s)\n", plan.moves.len(), plan.edit_counts.len());
    Ok(Report::new(Outcome::ok(data), human))
}

/// The paths under folder `from` that git lists but `plan` does not move.
fn left_behind(vault: &Vault, from: &str, plan: &Change) -> Result<Vec<String>, Refusal> {
    let (listed, _) = git::ls_files(&vault.root, Some(from)).map_err(Refusal::git)?;
    let left = listed
        .iter()
        // On the raw name: a submodule or a tracked path gone from disk may have a
        // non-UTF-8 name, which the vault check skips.
        .filter(|raw| vault.root.join(std::ffi::OsStr::from_bytes(raw)).symlink_metadata().is_ok())
        .map(|raw| String::from_utf8_lossy(raw).into_owned())
        .filter(|p| !plan.moves.contains_key(p))
        .collect();
    Ok(left)
}
