//! `loci closure` (m1-m4 §2).

use loci_core::closure;
use loci_core::envelope::{Diagnostic, Refusal, Report};
use loci_core::index::Index;
use loci_core::path::Kind;
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, concept: &str, depth: Option<usize>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let root = index.note_named(concept)?.of_kind(|k| k == Kind::Concept, "a concept")?;
    let c = closure::Closure::of(&index, root, depth);
    let unread = |p| {
        Diagnostic::warning("prereqs.unread", "its prerequisites cannot all be read; the closure may miss some")
            .at_path(p)
            .with_hint("`loci lint` names the problem")
    };
    let mut diags: Vec<Diagnostic> = c.unread.iter().map(unread).collect();
    if c.cyclic {
        diags.push(
            Diagnostic::warning(
                loci_core::lint::Code::PrereqsCycle,
                "the prerequisites form a cycle; the order breaks it by slug",
            )
            .at_path(&root.path),
        );
    }

    let human: String = c.members.iter().map(|m| format!("- [[{}]]\n", m.path.stem())).collect();
    let data = json!({
        "concept": root.path,
        "closure": c.members.iter().map(|m| json!({
            "slug": m.path.stem(), "path": m.path, "depth": m.depth,
        })).collect::<Vec<_>>(),
        "unresolved": c.unresolved.iter().map(|(f, t)| json!({"from": f, "target": t})).collect::<Vec<_>>(),
    });

    Ok(Report::ok(data, human).with_diagnostics(diags))
}
