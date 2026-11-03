//! Link rules (m0 §5): wikilinks only, and every link resolves.

use super::{Code, Ctx, managed};
use crate::envelope::{Applicability, Diagnostic, Fix};
use crate::id::Slug;
use crate::index::{Dangling, Index, LinkError};
use crate::parse::{FmChange, Link, LinkForm, MarkdownLink, Note, Spanned};
use crate::path::VaultPath;

pub(super) fn links(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in managed(ctx) {
        out.extend(note.markdown_links.iter().map(|m| markdown_link(note, m)));
        out.extend(note.links.iter().filter_map(|l| unresolved(ctx.index, note, l)));
    }
}

/// The unsafe fix pointing `link` at concept `slug` instead: a prereq is renamed with
/// its frontmatter (m0 §6.1), a body link in place with its fragment kept.
fn relink_fix(note: &Note, link: &Spanned<Link>, slug: &Slug) -> Fix {
    let fix = Fix::new(Applicability::Unsafe, format!("link [[{slug}]]"));
    match link.prereq_slug() {
        Some(from) => {
            fix.frontmatter(&note.path, FmChange::RenamePrereq { from: from.clone(), to: Some(slug.clone()) })
        }
        None => {
            let fragment = link.target.fragment.as_ref().map_or(String::new(), |f| format!("#{f}"));
            fix.edit(&note.path, link.target.span.clone(), format!("{slug}{fragment}"))
        }
    }
}

/// `link.markdown` for a markdown link to a vault file, with the wikilink that replaces
/// it as the fix.
fn markdown_link(note: &Note, m: &Spanned<MarkdownLink>) -> Diagnostic {
    // Notes are linked without `.md`; other files keep their extension.
    let url = m.url.strip_prefix("./").unwrap_or(&m.url);
    let target = if m.image { url } else { url.strip_suffix(".md").unwrap_or(url) };
    let bang = if m.image { "!" } else { "" };
    let wiki = format!("{bang}[[{target}]]");

    // The link text becomes the label, unless it only repeats the target.
    let label = m.label.text(&note.text).trim();
    let fixed = if label.is_empty() || label == target { wiki.clone() } else { format!("{bang}[[{target}|{label}]]") };
    // A percent-encoded URL (`a%20b.md`) names a file only once decoded: a guess.
    let applicability = if url.contains('%') { Applicability::Unsafe } else { Applicability::Safe };
    let fix = Fix::new(applicability, format!("write {fixed}")).edit(&note.path, m.span.clone(), fixed);

    Diagnostic::error(Code::LinkMarkdown, format!("a vault target must use a wikilink: `{wiki}`"))
        .in_note(note, m.span.clone())
        .with_hint(format!("write {wiki}"))
        .with_fix(fix)
}

/// The diagnostic for a link that does not resolve (§5.3, §5.4), if it does not.
fn unresolved(index: &Index, note: &Note, link: &Spanned<Link>) -> Option<Diagnostic> {
    let at = |code: Code, msg: String| Diagnostic::error(code, msg).in_note(note, link.target.span.clone());

    let target = target_name(note, link);
    let diag = match index.resolve(note, link).err()? {
        LinkError::PathForm => at(Code::LinkPathForm, "`..` or a leading `/` is not allowed in a link".into()),

        LinkError::Dangling(why) => {
            // A prereq is named by its slug, not its YAML string.
            let shown = if link.form == LinkForm::Prereq { target } else { link.target.text(&note.text) };
            let diag = at(Code::LinkDangling, format!("`{shown}` does not resolve"));
            match why {
                Dangling::Ambiguous(paths) => diag.with_hint(format!(
                    "several concepts are named `{target}` ({}); link by path",
                    paths.iter().map(VaultPath::as_str).collect::<Vec<_>>().join(", ")
                )),
                Dangling::NoConcept { closest: Some(slug) } => diag
                    .with_hint(format!("no concept is named `{target}`; did you mean [[{slug}]]?"))
                    .with_fix(relink_fix(note, link, &slug)),
                Dangling::NoConcept { closest: None } => diag.with_hint(format!(
                    "no concept is named `{target}`; create it with `loci new`, or link a non-concept by path"
                )),
                Dangling::NoFile => diag,
            }
        }

        // The real choices, so a renamed heading can be relinked without opening the
        // note; picking one is judgement, so there is no fix.
        LinkError::Heading { note: target_note, heading } => {
            let target_note = index.parsed(&target_note).expect("a heading is checked in a parsed note");
            let headings: Vec<&str> = target_note.headings.iter().map(|h| h.content.as_str()).collect();
            let hint = if headings.is_empty() {
                "it has no headings".to_string()
            } else {
                format!("its headings are: {}", headings.join(", "))
            };
            at(Code::LinkDanglingHeading, format!("`{target}` has no heading `{heading}`")).with_hint(hint)
        }

        LinkError::Block { id, .. } => at(Code::LinkDanglingBlock, format!("`{target}` has no block `^{id}`"))
            .with_hint(
                "the block was removed or its id changed; link the note, or another block by the `^id` that ends it",
            ),
    };

    Some(diag)
}

/// What `link` names, for messages: its slug or path, or the linking note's own name.
fn target_name<'a>(from: &'a Note, link: &'a Spanned<Link>) -> &'a str {
    link.target.names.name().unwrap_or_else(|| from.path.stem())
}
