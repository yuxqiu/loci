//! `loci mv` (m0 §9.5).

use loci_core::envelope::{Outcome, Refusal, Report};
use loci_core::index::Index;
use loci_core::vault;
use loci_core::{git, mv, relink};
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, from: &str, to: &str) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let from = vault::path_arg(from)?;
    let to = vault::path_arg(to)?;
    let index = Index::open(&vault);
    let to_ignored = git::is_ignored(&vault.root, &to).map_err(Refusal::git)?;
    let exists = |p: &str| vault.abs(p).symlink_metadata().is_ok();
    let plan = mv::plan(&index, &from, &to, to_ignored, exists)?;
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
