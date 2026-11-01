//! `loci merge` (m1-m4 §1).

use loci_core::envelope::{Outcome, Refusal, Report};
use loci_core::index::Index;
use loci_core::vault;
use loci_core::{merge, relink};
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, from: &str, into: &str) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let (from, into) = (vault::path_arg(from)?, vault::path_arg(into)?);
    let merge::Merge { from, into, change } = merge::plan(&index, &from, &into)?;
    relink::apply(&vault.root, &change).map_err(Refusal::io)?;
    let data = json!({
        "from": from,
        "into": into,
        "edited": change.edit_counts.iter().map(|(p, n)| json!({"path": p, "edits": n})).collect::<Vec<_>>(),
    });
    let human = format!("merged {from} into {into}; edited {} file(s)\n", change.edit_counts.len());
    Ok(Report::new(Outcome::ok(data), human))
}
