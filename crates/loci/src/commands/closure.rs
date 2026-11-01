//! `loci closure` (m1-m4 §2).

use loci_core::closure;
use loci_core::digest;
use loci_core::envelope::{Diagnostic, Refusal, Report};
use loci_core::index::Index;
use loci_core::vault;
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, slug: &str, depth: Option<usize>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let slug = vault::path_arg(slug)?;
    let path = index.concept(&slug).map_err(|e| e.refusal("closure.unknown", &slug))?.path.clone();
    let c = closure::closure(&index, &path, depth);
    let mut diags = vec![];
    if c.cyclic {
        diags.push(
            Diagnostic::warning("prereqs.cycle", "the prerequisites form a cycle; the order breaks it by slug")
                .at_path(path.as_str()),
        );
    }
    let human: String = c.members.iter().map(|m| digest::concept_line(&m.slug, m.digest.as_deref())).collect();
    let data = json!({
        "concept": path,
        "closure": c.members.iter().map(|m| json!({
            "slug": m.slug, "path": m.path, "depth": m.depth, "digest": m.digest,
        })).collect::<Vec<_>>(),
        "unresolved": c.unresolved.iter().map(|(f, t)| json!({"from": f, "target": t})).collect::<Vec<_>>(),
    });
    Ok(Report::ok(data, human).with_diagnostics(diags))
}
