//! Link rules (m0 §5): wikilinks only, and every link resolves.

use super::{Ctx, managed};
use crate::envelope::{Applicability, Diagnostic, Fix};
use crate::index::{Index, LinkError};
use crate::parse::{Link, LinkForm, MarkdownLink, Note, rename_prereqs, rewrite_frontmatter};
use crate::splice::Edit;

pub(super) fn links(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in managed(ctx) {
        out.extend(note.markdown_links.iter().map(|m| markdown_link(note, m)));
        out.extend(note.links.iter().filter_map(|l| unresolved(ctx.index, note, l)));
    }
}

/// `link.markdown` for a markdown link to a vault file, with the wikilink that replaces
/// it as the fix.
fn markdown_link(note: &Note, m: &MarkdownLink) -> Diagnostic {
    let (path, text) = (note.path.as_str(), note.text.as_str());
    // Notes are linked without `.md`; other files keep their extension.
    let url = m.url.strip_prefix("./").unwrap_or(&m.url);
    let target = if m.image { url } else { url.strip_suffix(".md").unwrap_or(url) };
    let bang = if m.image { "!" } else { "" };
    let wiki = format!("{bang}[[{target}]]");

    // The link text becomes the label, unless it only repeats the target.
    let label = text[m.label.clone()].trim();
    let fixed = if label.is_empty() || label == target { wiki.clone() } else { format!("{bang}[[{target}|{label}]]") };
    // A percent-encoded URL (`a%20b.md`) names a file only once decoded: a guess.
    let applicability = if url.contains('%') { Applicability::Unsafe } else { Applicability::Safe };
    let fix = Fix::new(applicability, format!("write {fixed}")).edit(path, m.span.clone(), fixed);

    Diagnostic::error("link.markdown", format!("a vault target must use a wikilink: `{wiki}`"))
        .at_path(path)
        .at(text, m.span.clone())
        .with_hint(format!("write {wiki}"))
        .with_fix(fix)
}

/// The diagnostic for a link that does not resolve (§5.3, §5.4), if it does not.
fn unresolved(index: &Index, note: &Note, link: &Link) -> Option<Diagnostic> {
    let at = |code: &str, msg: String| {
        Diagnostic::error(code, msg).at_path(note.path.as_str()).at(&note.text, link.target.span.clone())
    };

    let target = target_name(note, link);
    let diag = match index.resolve(note, link).err()? {
        LinkError::PathForm => at("link.path-form", "`..` or a leading `/` is not allowed in a link".into()),

        LinkError::Dangling { hint, closest } => {
            // A prereq is named by its slug, not its YAML string.
            let shown = if link.form == LinkForm::Prereq { target } else { link.written(&note.text) };
            let mut diag = at("link.dangling", format!("`{shown}` does not resolve"));
            diag.hint = hint;
            if let Some(slug) = closest {
                let fix = Fix::new(Applicability::Unsafe, format!("link [[{slug}]]"));
                // A prereq is renamed with its frontmatter (m0 §6.1).
                let edit = match link.form {
                    LinkForm::Prereq => {
                        rewrite_frontmatter(note, |m| rename_prereqs(m, &[(target, Some(slug.as_str()))].into()))
                    }
                    _ => {
                        let fragment = link.target.fragment.as_ref().map_or(String::new(), |f| format!("#{f}"));
                        Edit::replace(link.target.span.clone(), format!("{slug}{fragment}"))
                    }
                };
                diag = diag.with_fix(fix.edit(note.path.as_str(), edit.range, edit.new));
            }
            diag
        }

        // The real choices, so a renamed heading can be relinked without opening the
        // note; picking one is judgement, so there is no fix.
        LinkError::Heading => {
            let headings: Vec<&str> = index
                .link_target(note, link)
                .and_then(|p| index.parsed(&p))
                .iter()
                .flat_map(|t| &t.headings)
                .map(|h| h.text.as_str())
                .collect();
            let hint = if headings.is_empty() {
                "it has no headings".to_string()
            } else {
                format!("its headings are: {}", headings.join(", "))
            };
            at("link.dangling-heading", format!("`{target}` has no heading `{}`", fragment(link))).with_hint(hint)
        }

        LinkError::Block => at("link.dangling-block", format!("`{target}` has no block `{}`", fragment(link)))
            .with_hint(
                "the block was removed or its id changed; link the note, or another block by the `^id` that ends it",
            ),
    };

    Some(diag)
}

/// A link's fragment as written after the `#`.
fn fragment(link: &Link) -> String {
    link.target.fragment.as_ref().map(ToString::to_string).unwrap_or_default()
}

/// What `link` names, for messages: its slug or path, or the linking note's own name.
fn target_name<'a>(from: &'a Note, link: &'a Link) -> &'a str {
    link.target.names.name().unwrap_or_else(|| from.path.stem())
}
