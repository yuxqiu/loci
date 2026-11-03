//! `AREA.md` rules (m0 §6.3).

use super::Ctx;
use crate::envelope::Diagnostic;
use crate::parse::{BlockType, Note, TopBlock};
use crate::path::VaultPath;
use crate::vault::Kind;

/// The AREA.md rules: every area has one, an H1 and one paragraph within budget.
pub(super) fn areas(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for a in ctx.index.areas() {
        let p = VaultPath::new(format!("concepts/{a}/AREA.md")).expect("an area is a folder name");
        if ctx.index.parsed(&p).is_none() {
            out.push(Diagnostic::error("area.missing", format!("area `{a}` has no AREA.md")).at_path(p.as_str()));
        }
    }

    for note in ctx.index.notes().filter(|n| n.kind == Kind::Area) {
        let path = note.path.as_str();
        let Some(para) = area_paragraph(note) else {
            out.push(Diagnostic::error("area.format", "AREA.md must be an H1 and one paragraph").at_path(path));
            continue;
        };
        let n = para.chars().count();
        if n > ctx.budgets.area_paragraph {
            out.push(
                Diagnostic::warning(
                    "area.too-long",
                    format!("area paragraph is {n} characters (budget {})", ctx.budgets.area_paragraph),
                )
                .at_path(path),
            );
        }
    }
}

/// The paragraph of an AREA.md that is exactly an H1 and one paragraph (m0 §6.3).
fn area_paragraph(note: &Note) -> Option<&str> {
    if note.frontmatter.is_some() {
        return None;
    }
    match note.top_blocks.as_slice() {
        [TopBlock { kind: BlockType::Heading(1), .. }, TopBlock { kind: BlockType::Paragraph, span }] => {
            Some(note.text[span.clone()].trim())
        }
        _ => None,
    }
}
