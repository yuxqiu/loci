//! `loci links` (m1-m4 §5): the links of a note and the links pointing at it.

use loci_core::envelope::{Outcome, Report, line_col};
use loci_core::index::Index;
use serde_json::json;

use super::{Ctx, vault_path};

pub fn run(ctx: &Ctx, target: &str) -> Result<Report, Report> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let unknown = || Report::fail(Outcome::refused("links.unknown", format!("no note or file `{target}`")));
    // A bare name is a concept; anything else is a vault path (`.md` optional).
    let path = if target.contains('/') {
        let p = vault_path(target)?;
        let md = format!("{p}.md");
        if index.notes.contains_key(&md) {
            md
        } else if index.files.binary_search(&p).is_ok() {
            p
        } else {
            return Err(unknown());
        }
    } else {
        index
            .concept(target)
            .map_err(|e| {
                let (code, message) = e.refusal("links.unknown", target);
                Report::fail(Outcome::refused(code, message))
            })?
            .path
            .clone()
    };

    let mut human = format!("links from {path}:\n");
    let mut outgoing = vec![];
    if let Some(note) = index.notes.get(&path) {
        for link in &note.links {
            let line = line_col(&note.text, link.range.start).0;
            let to = index.link_target(note, link);
            human.push_str(&format!("  {line}: {} -> {}\n", link.raw, to.as_deref().unwrap_or("(unresolved)")));
            outgoing.push(json!({"line": line, "form": link.form, "raw": link.raw, "target": to}));
        }
    }
    human.push_str(&format!("links to {path}:\n"));
    let mut incoming = vec![];
    for (note, link) in index.incoming(&path) {
        let line = line_col(&note.text, link.range.start).0;
        human.push_str(&format!("  {}:{line}: {}\n", note.path, link.raw));
        incoming.push(json!({"path": note.path, "line": line, "form": link.form, "raw": link.raw}));
    }
    let data = json!({"note": path, "outgoing": outgoing, "incoming": incoming});
    Ok(Report::new(Outcome::ok(data), human))
}
