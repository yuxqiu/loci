//! `AREA.md` rules (m0 §6.3).

use super::Ctx;
use crate::config::Budgets;
use crate::envelope::Diagnostic;
use crate::parse::{Note, TopBlock};
use crate::path::VaultPath;
use crate::vault::Kind;

/// The AREA.md rules: every area has one, an H1 and one paragraph within budget.
pub(super) fn areas(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for a in ctx.index.areas() {
        let p = VaultPath::new(format!("concepts/{a}/AREA.md")).expect("an area is a folder name");
        if ctx.index.parsed(&p).is_none() {
            out.push(Diagnostic::error("area.missing", format!("area `{a}` has no AREA.md")).at_path(p));
        }
    }
    for note in ctx.index.notes().filter(|n| n.kind == Kind::Area) {
        area_rules(note, ctx.budgets, out);
    }
}

/// The paragraph of an AREA.md that is exactly an H1 and one paragraph (m0 §6.3).
fn area_paragraph(note: &Note) -> Option<&str> {
    if note.frontmatter.is_some() || note.fm_error.is_some() {
        return None;
    }
    match note.top_blocks.as_slice() {
        [(TopBlock::Heading(1), _), (TopBlock::Paragraph, p)] => Some(note.text[p.clone()].trim()),
        _ => None,
    }
}

fn area_rules(note: &Note, budgets: &Budgets, out: &mut Vec<Diagnostic>) {
    let path = note.path.as_str();
    match area_paragraph(note) {
        None => out.push(Diagnostic::error("area.format", "AREA.md must be an H1 and one paragraph").at_path(path)),
        Some(para) => {
            let n = para.chars().count();
            if n > budgets.area_paragraph {
                out.push(
                    Diagnostic::warning(
                        "area.too-long",
                        format!("area paragraph is {n} characters (budget {})", budgets.area_paragraph),
                    )
                    .at_path(path),
                );
            }
        }
    }
}
