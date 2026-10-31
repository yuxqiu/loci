//! `loci merge` (m1-m4 §1).

use loci_core::envelope::{Outcome, Report};
use loci_core::index::Index;
use loci_core::{merge, relink};
use serde_json::json;

use super::{Ctx, vault_path};

pub fn run(ctx: &Ctx, from: &str, into: &str) -> Result<Report, Report> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let from = vault_path(from)?;
    let into = vault_path(into)?;
    let change = merge::plan(&index, &from, &into).map_err(|e| Report::fail(Outcome::refused(e.code, e.message)))?;
    if let Err(e) = relink::apply(&vault.root, &change) {
        return Err(Report::fail(Outcome::internal("io", e.to_string())));
    }
    let concept = |arg: &str| index.concept(arg).expect("the plan resolved both concepts").path.clone();
    let (absorbed, target) = (concept(&from), concept(&into));
    let data = json!({
        "from": absorbed,
        "into": target,
        "edited": change.edit_counts.iter().map(|(p, n)| json!({"path": p, "edits": n})).collect::<Vec<_>>(),
        "aliases_added": change.aliases_added.iter().map(|(p, a)| json!({"path": p, "alias": a})).collect::<Vec<_>>(),
    });
    let human = format!("merged {absorbed} into {target}; edited {} file(s)\n", change.edit_counts.len());
    Ok(Report::new(Outcome::ok(data), human))
}
