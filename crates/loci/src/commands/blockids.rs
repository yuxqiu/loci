//! `loci block-ids` (m0 §9.9).

use loci_core::blockids;
use loci_core::envelope::{Refusal, Report};
use loci_core::index::Index;
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, note: &str, section: &str) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    // Like every note argument: a path (`.md` optional) or a concept slug (m0 §8).
    let index = Index::open(&vault);
    let parsed = index.note_named(note)?;
    if !parsed.kind.is_managed() {
        return Err(Refusal::refused("note.kind", format!("`{}` is not a managed note", parsed.path)));
    }

    let path = parsed.path.clone();
    let (out, assigned) = blockids::assign(parsed, section, loci_core::id::Ulid::monotonic())?;

    if !assigned.is_empty() {
        vault.write(&path, out.as_bytes())?;
    }

    let data = json!({
        "path": path,
        "section": section,
        "assigned": assigned.iter().map(|a| json!({"line": a.line, "id": a.id})).collect::<Vec<_>>(),
    });

    Ok(Report::ok(data, format!("assigned {} id(s) in {path}\n", assigned.len())))
}
