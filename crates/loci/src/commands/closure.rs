//! `loci closure` (m1-m4 §2).

use loci_core::closure;
use loci_core::envelope::{Diagnostic, Outcome, Refusal, Report};
use loci_core::index::Index;
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, slug: &str, depth: Option<usize>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let path = index.concept(slug).map_err(|e| e.refusal("closure.unknown", slug))?.path.clone();
    let c = closure::closure(&index, &path, depth);
    let mut diags = vec![];
    if c.cyclic {
        diags.push(
            Diagnostic::warning("prereqs.cycle", "the prerequisites form a cycle; the order breaks it by slug")
                .at_path(path.as_str()),
        );
    }
    let human: String = c
        .members
        .iter()
        .map(|m| match &m.digest {
            Some(d) => format!("- [[{}]]: {d}\n", m.slug),
            None => format!("- [[{}]]\n", m.slug),
        })
        .collect();
    let data = json!({
        "concept": path,
        "closure": c.members.iter().map(|m| json!({
            "slug": m.slug, "path": m.path, "depth": m.depth, "digest": m.digest,
        })).collect::<Vec<_>>(),
        "unresolved": c.unresolved.iter().map(|(f, t)| json!({"from": f, "target": t})).collect::<Vec<_>>(),
    });
    Ok(Report::new(Outcome::ok(data).with_diagnostics(diags), human))
}
