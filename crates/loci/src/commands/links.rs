//! `loci links` (m1-m4 §5): the links of a note and the links pointing at it.

use loci_core::envelope::{Refusal, Report, line_col};
use loci_core::index::Index;
use loci_core::parse::{LinkForm, Note};
use loci_core::path::VaultPath;
use serde_json::{Value, json};

use super::Ctx;

pub fn run(ctx: &Ctx, target: &str) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let target = VaultPath::from_arg(target)?;

    // A note or concept, else any other vault file (an image, a source file).
    let path = match index.note(&target, "links.unknown") {
        Ok(n) => n.path.clone(),
        Err(_) if index.contains(&target) => target,
        Err(r) => return Err(r),
    };

    let note = index.parsed(&path);
    let mut human = String::new();
    let outgoing = outgoing(&index, &path, note, &mut human);
    let incoming = incoming(&index, &path, &mut human);
    // Missing links, both ways: notes that name this one, and concepts this one names.
    let (unlinked_in, unlinked_out) = note.map_or_else(Default::default, |n| unlinked(&index, n, &mut human));

    let data = json!({"note": path, "outgoing": outgoing, "incoming": incoming, "unlinked_in": unlinked_in, "unlinked_out": unlinked_out});
    Ok(Report::ok(data, human))
}

/// The links of `note` (none for a file that is no note), each with its target.
fn outgoing(index: &Index, path: &VaultPath, note: Option<&Note>, human: &mut String) -> Vec<Value> {
    human.push_str(&format!("links from {path}:\n"));
    let Some(note) = note else { return vec![] };
    let mut rows = vec![];
    for link in &note.links {
        let (line, raw) = (line_col(&note.text, link.span.start).0, link.target.text(&note.text));
        let target = index.link_target(note, link);
        let to = target.as_ref().map_or("(unresolved)", VaultPath::as_str);
        human.push_str(&format!("  {line}: {raw} -> {to}{}\n", form_note(link.form)));
        rows.push(json!({"line": line, "form": link.form, "raw": raw, "target": target}));
    }
    rows
}

/// The links pointing at `path`.
fn incoming(index: &Index, path: &VaultPath, human: &mut String) -> Vec<Value> {
    human.push_str(&format!("links to {path}:\n"));
    let mut rows = vec![];
    for (note, link) in index.incoming(path) {
        let (line, raw) = (line_col(&note.text, link.span.start).0, link.target.text(&note.text));
        human.push_str(&format!("  {}:{line}: {raw}{}\n", note.path, form_note(link.form)));
        rows.push(json!({"path": note.path, "line": line, "form": link.form, "raw": raw}));
    }
    rows
}

/// Mentions without a link: of `note` in other notes, and of concepts in `note`.
fn unlinked(index: &Index, note: &Note, human: &mut String) -> (Vec<Value>, Vec<Value>) {
    human.push_str(&format!("unlinked mentions of {}:\n", note.path));
    let mut mentions_of = vec![];
    for (other, r) in index.unlinked_mentions_of(note) {
        let (line, text) = line_text(other, r.start);
        human.push_str(&format!("  {}:{line}: {text}\n", other.path));
        mentions_of.push(json!({"path": other.path, "line": line, "text": text}));
    }

    human.push_str(&format!("unlinked mentions in {}:\n", note.path));
    let mut mentions_in = vec![];
    for (concept, r) in index.unlinked_mentions_in(note) {
        let (line, words) = (line_col(&note.text, r.start).0, &note.text[r]);
        human.push_str(&format!("  {line}: {words} -> [[{}]]\n", concept.path.stem()));
        mentions_in.push(json!({"line": line, "text": words, "concept": concept.path}));
    }
    (mentions_of, mentions_in)
}

/// The 1-based line of byte offset `at` in `note`, and that line, trimmed.
fn line_text(note: &Note, at: usize) -> (usize, &str) {
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
