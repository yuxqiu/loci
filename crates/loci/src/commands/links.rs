//! `loci links` (m1-m4 §5): the links of a note and the links pointing at it.

use loci_core::envelope::{Outcome, Refusal, Report, line_col};
use loci_core::index::Index;
use loci_core::vault;
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, target: &str) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let target = vault::path_arg(target)?;
    // A note or concept, else any other vault file (an image, a source file).
    let path = match index.note(&target) {
        Ok(n) => n.path.clone(),
        Err(e) if index.files.binary_search(&target).is_err() => return Err(e.refusal("links.unknown", &target)),
        Err(_) => target,
    };

    let mut human = format!("links from {path}:\n");
    let mut outgoing = vec![];
    if let Some(note) = index.markdown.get(&path) {
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
