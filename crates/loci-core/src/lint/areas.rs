//! `AREA.md` rules (m0 §6.3).

use super::{Code, Ctx};
use crate::envelope::Diagnostic;
use crate::parse::{BlockType, Note, Spanned};
use crate::path::{AREA, Kind, VaultPath};

/// The AREA.md rules: every area has one, an H1 and one paragraph within budget.
pub(super) fn areas(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for a in ctx.index.areas() {
        let p = VaultPath::new(format!("concepts/{a}/{AREA}")).expect("an area is a folder name");
        if ctx.index.parsed(&p).is_none() {
            out.push(Diagnostic::error(Code::AreaMissing, format!("area `{a}` has no AREA.md")).at_path(&p));
        }
    }

    for note in ctx.index.notes().filter(|n| n.kind == Kind::Area) {
        let path = &note.path;
        let Some(para) = area_paragraph(note) else {
            out.push(Diagnostic::error(Code::AreaFormat, "AREA.md must be an H1 and one paragraph").at_path(path));
            continue;
        };
        let n = para.chars().count();
        if n > ctx.budgets.area_paragraph {
            out.push(
                Diagnostic::warning(
                    Code::AreaTooLong,
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
        [Spanned { node: BlockType::Heading(1), .. }, Spanned { node: BlockType::Paragraph, span }] => {
            Some(note.text[span.clone()].trim())
        }
        _ => None,
    }
}
