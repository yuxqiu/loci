//! Rules every managed note follows (m0 §4, §7.3): a byte-order mark, frontmatter where it
//! belongs, the H1, unique block ids, and the size budget.

use std::collections::BTreeSet;

use super::{Code, Ctx, managed};
use crate::envelope::{Applicability, Diagnostic, Fix};
use crate::id::Ulid;
use crate::parse::{BOM, Block, Frontmatter};
use crate::path::Kind;

/// A byte-order mark, broken frontmatter, and frontmatter outside concepts.
pub(super) fn note_format(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in managed(ctx) {
        let path = &note.path;
        if note.has_bom() {
            let fix = Fix::new(Applicability::Safe, "delete the byte-order mark").edit(path, 0..BOM.len(), "");
            out.push(
                Diagnostic::error(Code::FileBom, "file starts with a byte-order mark").at_path(path).with_fix(fix),
            );
        }
        let Some(fm) = &note.frontmatter else { continue };
        match &fm.node {
            Frontmatter::Unterminated => out.push(
                Diagnostic::error(Code::FmUnterminated, "frontmatter opened with `---` is not a closed block")
                    .at_path(path),
            ),
            Frontmatter::Invalid(reason) => out.push(
                Diagnostic::error(Code::FmYaml, format!("frontmatter is not a YAML mapping: {reason}"))
                    .in_note(note, fm.span.clone()),
            ),
            Frontmatter::Fields { .. } => {}
        }
        if fm.is_closed() && note.kind != Kind::Concept {
            out.push(
                Diagnostic::error(Code::FmNotAllowed, format!("{} files have no frontmatter", note.kind))
                    .in_note(note, fm.span.clone()),
            );
        }
    }
}

pub(super) fn note_size(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in ctx.index.concepts() {
        let body_chars = note.text[note.body_start()..].chars().count();
        if body_chars > ctx.budgets.note {
            out.push(
                Diagnostic::warning(
                    Code::SizeNote,
                    format!("note body is {body_chars} characters (budget {})", ctx.budgets.note),
                )
                .at_path(&note.path),
            );
        }
    }
}

/// Block ids unique within a note.
pub(super) fn block_ids(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in managed(ctx) {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for b in note.block_ids() {
            if !seen.insert(b.id.value(&note.text)) {
                // Unsafe: a link may mean this occurrence.
                let fix = Fix::new(Applicability::Unsafe, "give this block a new id").edit(
                    &note.path,
                    b.id.span.clone(),
                    format!("^{}", Ulid::generate()),
                );
                out.push(
                    Diagnostic::error(
                        Code::BlockIdDuplicate,
                        format!("block id `^{}` appears more than once in this note", b.id.value(&note.text)),
                    )
                    .in_note(note, b.id.span.clone())
                    .with_fix(fix),
                );
            }
        }
    }
}

pub(super) fn h1(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in managed(ctx) {
        let path = &note.path;
        let h1s: Vec<_> = note.headings().filter(|h| h.level == 1).collect();
        if h1s.is_empty() {
            let at = note.body_start();
            let fix = Fix::new(Applicability::DisplayOnly, "add a title").edit(
                path,
                at..at,
                format!("# {}\n\n", note.path.stem()),
            );
            out.push(Diagnostic::error(Code::H1Missing, "no H1").at_path(path).with_fix(fix));
            continue;
        }
        if h1s.len() > 1 {
            out.push(Diagnostic::error(Code::H1Multiple, "more than one H1").in_note(note, h1s[1].span.clone()));
        }
        if !note.body.first().is_some_and(|b| matches!(&b.node, Block::Heading(h) if h.level == 1)) {
            out.push(
                Diagnostic::error(Code::H1NotFirst, "the H1 is not the first block after the frontmatter")
                    .in_note(note, h1s[0].span.clone()),
            );
        }
    }
}
