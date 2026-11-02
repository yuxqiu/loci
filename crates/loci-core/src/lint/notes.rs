//! Rules every managed note follows (m0 §4, §7.3): a byte-order mark, frontmatter where it
//! belongs, the H1, unique block ids, and the size budget.

use std::collections::BTreeSet;

use super::{Ctx, managed};
use crate::envelope::{Applicability, Diagnostic, Fix};
use crate::id::Ulid;
use crate::parse::{BOM, FmError, Note, TopBlock};
use crate::vault::Kind;

/// A byte-order mark, broken frontmatter, and frontmatter outside concepts.
pub(super) fn note_format(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in managed(ctx) {
        let path = note.path.as_str();
        let text = note.text.as_str();
        let d = |code: &str, msg: String| Diagnostic::error(code, msg).at_path(path);
        if note.bom {
            let fix = Fix::new(Applicability::Safe, "delete the byte-order mark").edit(path, 0..BOM.len(), "");
            out.push(d("file.bom", "file starts with a byte-order mark".into()).with_fix(fix));
        }
        match &note.fm_error {
            Some(FmError::Unterminated) => {
                out.push(d("fm.unterminated", "frontmatter opened with `---` is not a closed block".into()))
            }
            Some(FmError::Yaml(reason)) => {
                let r = note.frontmatter.as_ref().map_or(0..0, |f| f.range.clone());
                out.push(d("fm.yaml", format!("frontmatter is not a YAML mapping: {reason}")).at(text, r));
            }
            None => {}
        }
        if let Some(fm) = note.frontmatter.as_ref().filter(|_| note.kind != Kind::Concept) {
            out.push(
                d("fm.not-allowed", format!("{} files have no frontmatter", note.kind.name()))
                    .at(text, fm.range.clone()),
            );
        }
    }
}

pub(super) fn h1(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in managed(ctx) {
        h1_rules(note, out);
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
                    b.range.clone(),
                    format!("^{}", Ulid::generate()),
                );
                out.push(
                    Diagnostic::error(
                        "block.id-duplicate",
                        format!("block id `^{}` appears more than once in this note", b.id),
                    )
                    .at_path(note.path.as_str())
                    .at(&note.text, b.range.clone())
                    .with_fix(fix),
                );
            }
        }
    }
}

fn h1_rules(note: &Note, out: &mut Vec<Diagnostic>) {
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
        return;
    }
    if h1s.len() > 1 {
        out.push(
            Diagnostic::error("h1.multiple", "more than one H1").at_path(path).at(&note.text, h1s[1].range.clone()),
        );
    }
    if !matches!(note.top_blocks.first(), Some((TopBlock::Heading(1), _))) {
        out.push(
            Diagnostic::error("h1.not-first", "the H1 is not the first block after the frontmatter")
                .at_path(path)
                .at(&note.text, h1s[0].range.clone()),
        );
    }
}
