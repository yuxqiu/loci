//! `loci mv` (m0 §9.5).

use loci_core::envelope::{Refusal, Report};
use loci_core::index::Index;
use loci_core::path::VaultPath;

use loci_core::relink::Change;
use loci_core::vault::Vault;
use loci_core::{git, mv, relink};
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, from: &str, to: &str, dry_run: bool) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let from = VaultPath::from_arg(from)?;
    let to = VaultPath::from_arg(to)?;
    let index = Index::open(&vault);
    let from = mv::stored_name(&index, &from)?;
    let exists = |p: &VaultPath| vault.abs(p).symlink_metadata().is_ok();
    let ignored = |p: &VaultPath| git::is_ignored(&vault.root, p).map_err(Refusal::git);
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
    if let Some(p) = plan.moves.keys().find(|p| p.is_source_file() && changed.contains(p)) {
        return Err(Refusal::refused(
            "mv.source-changed",
            format!("source `{p}` differs from its committed version; restore it before moving it"),
        ));
    }
    if !dry_run {
        relink::apply(&vault, &plan).map_err(Refusal::io)?;
    }
    let data = json!({
        "moved": plan.moves.iter().collect::<Vec<_>>(),
        "edited": plan.writes.iter().map(|(p, w)| json!({"path": p, "edits": w.edits})).collect::<Vec<_>>(),
        "dry_run": dry_run,
    });
    let (moved, edited) = if dry_run { ("would move", "edit") } else { ("moved", "edited") };
    let mut human = format!("{moved} {} file(s), {edited} {} file(s)\n", plan.moves.len(), plan.writes.len());
    if dry_run {
        human.push_str(&super::plan_lines(&plan));
    }
    Ok(Report::ok(data, human))
}

/// The paths under folder `from` that git lists, that exist on disk, and that `plan`
/// does not move (a submodule, a nested repository's folder). Every name is UTF-8: the
/// vault was refused at open otherwise; a name that is not (the vault changed since)
/// is left behind too.
fn left_behind(vault: &Vault, from: &VaultPath, plan: &Change) -> Result<Vec<String>, Refusal> {
    let (listed, _) = git::ls_files(&vault.root, Some(from)).map_err(Refusal::git)?;
    let mut left = vec![];
    for name in listed {
        match name {
            Ok(name)
                if vault.root.join(&name).symlink_metadata().is_ok() && !plan.moves.contains_key(name.as_str()) =>
            {
                left.push(name)
            }
            Ok(_) => {}
            Err(raw) => left.push(String::from_utf8_lossy(&raw).into_owned()),
        }
    }
    Ok(left)
}
