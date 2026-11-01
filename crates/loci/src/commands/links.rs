//! `loci links` (m1-m4 §5): the links of a note and the links pointing at it.

use loci_core::envelope::{Refusal, Report, line_col};
use loci_core::index::Index;
use loci_core::parse::LinkForm;
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
    if let Some(note) = index.parsed(&path) {
        for link in &note.links {
            let line = line_col(&note.text, link.range.start).0;
            let target = index.link_target(note, link);
            let to = target.as_deref().unwrap_or("(unresolved)");
            human.push_str(&format!("  {line}: {} -> {to}{}\n", link.raw, form_note(link.form)));
            outgoing.push(json!({"line": line, "form": link.form, "raw": link.raw, "target": target}));
        }
    }
    human.push_str(&format!("links to {path}:\n"));
    let mut incoming = vec![];
    for (note, link) in index.incoming(&path) {
        let line = line_col(&note.text, link.range.start).0;
        human.push_str(&format!("  {}:{line}: {}{}\n", note.path, link.raw, form_note(link.form)));
        incoming.push(json!({"path": note.path, "line": line, "form": link.form, "raw": link.raw}));
    }
    // Notes that name this one without linking it: candidates for a link.
    let mut unlinked = vec![];
    if let Some(note) = index.parsed(&path) {
        human.push_str(&format!("unlinked mentions of {path}:\n"));
        for (other, at) in index.unlinked_mentions(note) {
            let line = line_col(&other.text, at).0;
            let text = other.text.lines().nth(line - 1).unwrap_or_default().trim();
            human.push_str(&format!("  {}:{line}: {text}\n", other.path));
            unlinked.push(json!({"path": other.path, "line": line, "text": text}));
        }
    }
    let data = json!({"note": path, "outgoing": outgoing, "incoming": incoming, "unlinked": unlinked});
    Ok(Report::ok(data, human))
}

/// A marker for links that are not body wikilinks.
fn form_note(form: LinkForm) -> &'static str {
    match form {
        LinkForm::Prereq => " (prereq)",
        _ => "",
    }
}
