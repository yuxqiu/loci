//! `loci block-ids` (m0 §9.10).

use loci_core::blockids;
use loci_core::envelope::{Outcome, Report};
use loci_core::parse;
use loci_core::vault::{self, write_atomic};
use serde_json::json;

use super::{Ctx, vault_path};

pub fn run(ctx: &Ctx, note: &str, section: &str) -> Result<Report, Report> {
    let vault = ctx.vault()?;
    let path = vault_path(note)?;
    let kind = vault::kind_of(&path);
    if !kind.is_some_and(|k| k.is_managed()) {
        return Err(Report::fail(Outcome::refused("block-ids.kind", format!("`{path}` is not a managed note"))));
    }
    let abs = vault.abs(&path);
    let text = std::fs::read_to_string(&abs)
        .map_err(|e| Report::fail(Outcome::refused("block-ids.kind", format!("cannot read `{path}`: {e}"))))?;
    let parsed = parse::parse(&path, kind.expect("checked above"), text);
    let Some((out, assigned)) = blockids::assign(&parsed, section, blockids::ulid_ids()) else {
        return Err(Report::fail(Outcome::refused("block-ids.no-section", format!("`{path}` has no `## {section}`"))));
    };
    if !assigned.is_empty()
        && let Err(e) = write_atomic(&abs, out.as_bytes())
    {
        return Err(Report::fail(Outcome::internal("io", e.to_string())));
    }
    let data = json!({
        "path": path,
        "section": section,
        "assigned": assigned.iter().map(|a| json!({"line": a.line, "id": a.id})).collect::<Vec<_>>(),
    });
    Ok(Report::new(Outcome::ok(data), format!("assigned {} id(s) in {path}\n", assigned.len())))
}
