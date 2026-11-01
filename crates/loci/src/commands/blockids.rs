//! `loci block-ids` (m0 §9.9).

use loci_core::blockids;
use loci_core::envelope::{Outcome, Refusal, Report};
use loci_core::parse;
use loci_core::vault::{self, write_atomic};
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, note: &str, section: &str) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let path = vault::path_arg(note)?;
    let kind = vault::kind_of(&path);
    if !kind.is_some_and(|k| k.is_managed()) {
        return Err(Refusal::refused("block-ids.kind", format!("`{path}` is not a managed note")));
    }
    let abs = vault.abs(&path);
    let text = match std::fs::read_to_string(&abs) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Refusal::refused("block-ids.kind", format!("`{path}` does not exist")));
        }
        Err(e) => return Err(Refusal::io(format!("reading `{path}`: {e}"))),
    };
    let parsed = parse::parse(&path, kind.expect("checked above"), text);
    let Some((out, assigned)) = blockids::assign(&parsed, section, blockids::ulid_ids()) else {
        return Err(Refusal::refused("block-ids.no-section", format!("`{path}` has no `## {section}`")));
    };
    if !assigned.is_empty()
        && let Err(e) = write_atomic(&abs, out.as_bytes())
    {
        return Err(Refusal::io(e));
    }
    let data = json!({
        "path": path,
        "section": section,
        "assigned": assigned.iter().map(|a| json!({"line": a.line, "id": a.id})).collect::<Vec<_>>(),
    });
    Ok(Report::new(Outcome::ok(data), format!("assigned {} id(s) in {path}\n", assigned.len())))
}
