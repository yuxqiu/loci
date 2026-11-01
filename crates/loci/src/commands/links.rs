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
    // Missing links, both ways: notes that name this one, and concepts this one names.
    let (mut unlinked_in, mut unlinked_out) = (vec![], vec![]);
    if let Some(note) = index.parsed(&path) {
        human.push_str(&format!("unlinked mentions of {path}:\n"));
        for (other, r) in index.unlinked_mentions_of(note) {
            let (line, text) = line_text(other, r.start);
            human.push_str(&format!("  {}:{line}: {text}\n", other.path));
            unlinked_in.push(json!({"path": other.path, "line": line, "text": text}));
        }
        human.push_str(&format!("unlinked mentions in {path}:\n"));
        for (concept, r) in index.unlinked_mentions_in(note) {
            let line = line_col(&note.text, r.start).0;
            let words = &note.text[r];
            let slug = loci_core::vault::stem(&concept.path);
            human.push_str(&format!("  {line}: {words} -> [[{slug}]]\n"));
            unlinked_out.push(json!({"line": line, "text": words, "concept": concept.path}));
        }
    }
    let data = json!({"note": path, "outgoing": outgoing, "incoming": incoming, "unlinked_in": unlinked_in, "unlinked_out": unlinked_out});
    Ok(Report::ok(data, human))
}

/// The 1-based line of byte offset `at` in `note`, and that line, trimmed.
fn line_text(note: &loci_core::parse::Note, at: usize) -> (usize, &str) {
    let line = line_col(&note.text, at).0;
    (line, note.text.lines().nth(line - 1).unwrap_or_default().trim())
}

/// A marker for links that are not body wikilinks.
fn form_note(form: LinkForm) -> &'static str {
    match form {
        LinkForm::Prereq => " (prereq)",
        _ => "",
    }
}
