//! `loci mv` (m0 §9.5).

use loci_core::envelope::{Refusal, Report};
use loci_core::git;
use loci_core::index::Index;
use loci_core::path::VaultPath;

use loci_core::mv;
use loci_core::relink::Change;
use loci_core::vault::Vault;
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, from: &str, to: &str, dry_run: bool) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let from = VaultPath::from_arg(from)?;
    let to = VaultPath::from_arg(to)?;
    let index = Index::open(&vault);
    let from = mv::stored_name(&index, &from)?;
    let exists = |p: &VaultPath| vault.abs(p).symlink_metadata().is_ok();
    let ignored = |p: &VaultPath| vault.repo().is_ignored(p).map_err(Refusal::git);
    let plan = mv::plan(&index, &from, &to, ignored, exists)?;

    refuse_partial_folder(&vault, &from, &plan)?;
    refuse_changed_source(&vault, &plan)?;

    if !dry_run {
        plan.apply(&vault).map_err(Refusal::io)?;
    }

    let data = json!({
        "moved": plan.moves.iter().collect::<Vec<_>>(),
        "edited": super::edited(&plan),
        "dry_run": dry_run,
    });

    let (moved, edited) = if dry_run { ("would move", "edit") } else { ("moved", "edited") };
    let mut human = format!("{moved} {} file(s), {edited} {} file(s)\n", plan.moves.len(), plan.writes.len());
    if dry_run {
        human.push_str(&super::plan_lines(&plan));
    }

    Ok(Report::ok(data, human))
}

/// A folder moves whole: refused when git lists anything under `from` that is on disk
/// and that the plan does not move (a submodule, an untracked nested repository), or a
/// name that is not UTF-8 (the vault changed since it was opened). The plan renames a
/// moved file under its own path, and a folder's files under theirs.
fn refuse_partial_folder(vault: &Vault, from: &VaultPath, plan: &Change) -> Result<(), Refusal> {
    if plan.moves.contains_key(from) {
        return Ok(()); // A file.
    }
    let listing = vault.repo().ls_files(Some(from)).map_err(Refusal::git)?;
    let on_disk = |name: &String| vault.abs(&git::vault_path(name.clone())).symlink_metadata().is_ok();
    let mut left: Vec<String> =
        listing.names.into_iter().filter(|n| on_disk(n) && !plan.moves.contains_key(n.as_str())).collect();
    left.extend(listing.not_utf8);
    if left.is_empty() {
        return Ok(());
    }
    let message = format!("`{from}` holds files loci does not move: {}; move them with git first", left.join(", "));
    Err(Refusal::refused("mv.from", message))
}

/// Sources never change (m0 §6.4), and `sources.changed` sees an edit only at the same
/// path: git reports a rename as a deletion plus an addition. So a source with
/// uncommitted edits (in HEAD and on disk, but different) is not moved: the move would
/// carry the edit past lint as a plain rename.
fn refuse_changed_source(vault: &Vault, plan: &Change) -> Result<(), Refusal> {
    let changed = vault.repo().status().map_err(Refusal::git)?.modified;
    match plan.moves.keys().find(|p| p.is_source_file() && changed.contains(p)) {
        None => Ok(()),
        Some(p) => Err(Refusal::refused(
            "mv.source-changed",
            format!("source `{p}` differs from its committed version; restore it before moving it"),
        )),
    }
}
