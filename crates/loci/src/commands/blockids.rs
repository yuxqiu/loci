//! `loci block-ids` (m0 §9.9).

use loci_core::blockids;
use loci_core::envelope::{Refusal, Report};
use loci_core::index::Index;
use loci_core::path::VaultPath;
use loci_core::vault::write_atomic;
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, note: &str, section: &str) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let arg = VaultPath::from_arg(note)?;
    // Like every note argument: a path (`.md` optional) or a concept slug (m0 §9.9).
    let index = Index::open(&vault);
    let parsed = index.note(&arg, "block-ids.kind")?;
    if !parsed.kind.is_managed() {
        return Err(Refusal::refused("block-ids.kind", format!("`{}` is not a managed note", parsed.path)));
    }
    let path = parsed.path.clone();
    let Some((out, assigned)) = blockids::assign(parsed, section, loci_core::id::Ulid::monotonic()) else {
        return Err(Refusal::refused("block-ids.no-section", format!("`{path}` has no `## {section}`")));
    };
    if !assigned.is_empty() {
        write_atomic(&vault.abs(&path), out.as_bytes()).map_err(Refusal::io)?;
    }
    let data = json!({
        "path": path,
        "section": section,
        "assigned": assigned.iter().map(|a| json!({"line": a.line, "id": a.id})).collect::<Vec<_>>(),
    });
    Ok(Report::ok(data, format!("assigned {} id(s) in {path}\n", assigned.len())))
}
