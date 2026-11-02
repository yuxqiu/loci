//! Link rules (m0 §5): wikilinks only, and every link resolves.

use super::{Ctx, managed};
use crate::envelope::{Applicability, Diagnostic, Fix};
use crate::index::LinkError;
use crate::parse::Note;

pub(super) fn links(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    let index = ctx.index;
    for note in managed(ctx) {
        let path = note.path.as_str();
        let text = note.text.as_str();
        for m in &note.md_links {
            // Notes are linked without `.md`; other files keep their extension.
            let url = m.url.strip_prefix("./").unwrap_or(&m.url);
            let target = if m.image { url } else { url.strip_suffix(".md").unwrap_or(url) };
            let bang = if m.image { "!" } else { "" };
            let wiki = format!("{bang}[[{target}]]");
            // The link text becomes the label, unless it only repeats the target.
            let label = text[m.label.clone()].trim();
            let fixed =
                if label.is_empty() || label == target { wiki.clone() } else { format!("{bang}[[{target}|{label}]]") };
            // A percent-encoded URL (`a%20b.md`) names a file only once decoded: a guess.
            let applicability = if url.contains('%') { Applicability::Unsafe } else { Applicability::Safe };
            out.push(
                Diagnostic::error("link.markdown", format!("a vault target must use a wikilink: `{wiki}`"))
                    .at_path(path)
                    .at(text, m.range.clone())
                    .with_hint(format!("write {wiki}"))
                    .with_fix(Fix::new(applicability, format!("write {fixed}")).edit(
                        path,
                        m.range.clone(),
                        fixed.clone(),
                    )),
            );
        }
        for link in &note.links {
            let at = |code: &str, msg: String| {
                Diagnostic::error(code, msg).at_path(path).at(text, link.target_range.clone())
            };
            match index.resolve(note, link) {
                Ok(_) => {}
                Err(LinkError::Dangling { hint, closest }) => {
                    let mut diag = at("link.dangling", format!("`{}` does not resolve", link.raw));
                    diag.hint = hint;
                    if let Some(slug) = closest {
                        let new =
                            format!("{slug}{}", link.fragment.as_ref().map_or(String::new(), |f| format!("#{f}")));
                        let fix = Fix::new(Applicability::Unsafe, format!("link [[{slug}]]"));
                        diag = diag.with_fix(fix.edit(path, link.target_range.clone(), new));
                    }
                    out.push(diag);
                }
                Err(LinkError::PathForm) => {
                    out.push(at("link.path-form", "`..` or a leading `/` is not allowed in a link".into()))
                }
                Err(LinkError::Heading) => {
                    let diag = at(
                        "link.dangling-heading",
                        format!("`{}` has no heading `{}`", target_name(note, link), fragment(link)),
                    );
                    // The real choices, so a renamed heading can be relinked without
                    // opening the note; picking one is judgement, so there is no fix.
                    let target = index.link_target(note, link).and_then(|p| index.parsed(&p));
                    let headings: Vec<&str> =
                        target.iter().flat_map(|t| &t.headings).map(|h| h.text.as_str()).collect();
                    let hint = if headings.is_empty() {
                        "it has no headings".to_string()
                    } else {
                        format!("its headings are: {}", headings.join(", "))
                    };
                    out.push(diag.with_hint(hint))
                }
                Err(LinkError::Block) => out.push(
                    at("link.dangling-block", format!("`{}` has no block `{}`", target_name(note, link), fragment(link)))
                        .with_hint(
                            "the block was removed or its id changed; link the note, or another block by the `^id` that ends it",
                        ),
                ),
            }
        }
    }
}

/// A link's fragment as written after the `#`.
fn fragment(link: &crate::parse::Link) -> String {
    link.fragment.as_ref().map(ToString::to_string).unwrap_or_default()
}

fn target_name(from: &Note, link: &crate::parse::Link) -> String {
    if link.target.is_empty() { from.path.stem().to_string() } else { link.target.clone() }
}
