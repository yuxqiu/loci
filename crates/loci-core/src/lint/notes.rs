//! Rules every managed note follows (m0 §4, §7.3): a byte-order mark, frontmatter where it
//! belongs, the H1, unique block ids, and the size budget.

use std::collections::BTreeSet;

use super::{Ctx, managed};
use crate::envelope::{Applicability, Diagnostic, Fix};
use crate::id::Ulid;
use crate::parse::{BOM, BlockType, Frontmatter};
use crate::vault::Kind;

/// A byte-order mark, broken frontmatter, and frontmatter outside concepts.
pub(super) fn note_format(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in managed(ctx) {
        let path = note.path.as_str();
        let text = note.text.as_str();
        let d = |code: &str, msg: String| Diagnostic::error(code, msg).at_path(path);
        if note.has_bom() {
            let fix = Fix::new(Applicability::Safe, "delete the byte-order mark").edit(path, 0..BOM.len(), "");
            out.push(d("file.bom", "file starts with a byte-order mark".into()).with_fix(fix));
        }
        match &note.frontmatter {
            Some(Frontmatter::Unterminated) => {
                out.push(d("fm.unterminated", "frontmatter opened with `---` is not a closed block".into()))
            }
            Some(Frontmatter::Invalid { span, reason }) => {
                out.push(d("fm.yaml", format!("frontmatter is not a YAML mapping: {reason}")).at(text, span.clone()));
            }
            Some(Frontmatter::Fields { .. }) | None => {}
        }
        if let Some(span) = note.frontmatter.as_ref().and_then(Frontmatter::span).filter(|_| note.kind != Kind::Concept)
        {
            out.push(d("fm.not-allowed", format!("{} files have no frontmatter", note.kind.name())).at(text, span));
        }
    }
}

pub(super) fn note_size(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in ctx.index.concepts() {
        let body_chars = note.text[note.body_start()..].chars().count();
        if body_chars > ctx.budgets.note {
            out.push(
                Diagnostic::warning(
                    "size.note",
                    format!("note body is {body_chars} characters (budget {})", ctx.budgets.note),
                )
                .at_path(note.path.as_str()),
            );
        }
    }
}

/// Block ids unique within a note.
pub(super) fn block_ids(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in managed(ctx) {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for b in &note.block_ids {
            if !seen.insert(b.id.as_str()) {
                // Unsafe: a link may mean this occurrence.
                let fix = Fix::new(Applicability::Unsafe, "give this block a new id").edit(
                    note.path.as_str(),
                    b.span.clone(),
                    format!("^{}", Ulid::generate()),
                );
                out.push(
                    Diagnostic::error(
                        "block.id-duplicate",
                        format!("block id `^{}` appears more than once in this note", b.id),
                    )
                    .at_path(note.path.as_str())
                    .at(&note.text, b.span.clone())
                    .with_fix(fix),
                );
            }
        }
    }
}

pub(super) fn h1(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in managed(ctx) {
        let path = note.path.as_str();
        let h1s: Vec<_> = note.headings.iter().filter(|h| h.level == 1).collect();
        if h1s.is_empty() {
            let at = note.body_start();
            let fix = Fix::new(Applicability::DisplayOnly, "add a title").edit(
                path,
                at..at,
                format!("# {}\n\n", note.path.stem()),
            );
            out.push(Diagnostic::error("h1.missing", "no H1").at_path(path).with_fix(fix));
            continue;
        }
        if h1s.len() > 1 {
            out.push(
                Diagnostic::error("h1.multiple", "more than one H1").at_path(path).at(&note.text, h1s[1].span.clone()),
            );
        }
        if !note.top_blocks.first().is_some_and(|b| b.kind == BlockType::Heading(1)) {
            out.push(
                Diagnostic::error("h1.not-first", "the H1 is not the first block after the frontmatter")
                    .at_path(path)
                    .at(&note.text, h1s[0].span.clone()),
            );
        }
    }
}
