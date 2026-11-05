//! Link rules (m0 §5): wikilinks only, and every link resolves.

use super::{Code, Ctx, managed};
use crate::envelope::{Applicability, Diagnostic, Fix};
use crate::id::Slug;
use crate::index::{Dangling, Index, LinkError};
use crate::parse::{FmChange, Fragment, Link, LinkForm, MarkdownLink, Note, Spanned};
use crate::path::VaultPath;
use crate::vault::Vault;

pub(super) fn links(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in managed(ctx) {
        out.extend(note.markdown_links.iter().map(|m| markdown_link(ctx.index.vault(), note, m)));
        out.extend(note.links.iter().filter_map(|l| unresolved(ctx.index, note, l)));
    }
}

/// The headings of `note`, the real choices for a heading link that misses, so it can
/// be relinked without opening the note; picking one is judgement, so there is no fix.
fn headings_hint(note: &Note) -> String {
    let headings: Vec<_> = note.headings().map(|h| h.heading_text(&note.text)).collect();
    if headings.is_empty() {
        "it has no headings".to_string()
    } else {
        format!("its headings are: {}", headings.join(", "))
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
fn markdown_link(vault: &Vault, note: &Note, m: &Spanned<MarkdownLink>) -> Diagnostic {
    let at = |message: String, hint: String| {
        Diagnostic::error(Code::LinkMarkdown, message).in_note(note, m.span.clone()).with_hint(hint)
    };
    let resolved = vault.resolve(m.path());
    // No path link names a root file (§5.3), so it has no wikilink to suggest.
    if let Some(p) = resolved.filter(|p| p.parent().is_none()) {
        let hint = "no wikilink names a root file: move it into a folder (`loci mv`) or drop the link";
        return at(format!("a vault target must use a wikilink, and `{p}` is a root file"), hint.to_string());
    }
    // The shortest wikilink target for the file the URL names; as written if it names none.
    let target = resolved.map_or_else(|| m.path().to_string(), |path| crate::relink::written(vault, path));
    let bang = if m.image { "!" } else { "" };
    let wiki = format!("{bang}[[{target}]]");
    let diag = at(format!("a vault target must use a wikilink: `{wiki}`"), format!("write {wiki}"));

    // The link text becomes the label, unless it only repeats the target. No fix for a
    // URL that names no file as written (a percent-encoded one included), or a label a
    // wikilink cannot hold (`]]`, or a final `]`).
    let label = m.label.value(&note.text).trim();
    if resolved.is_none() || label.contains("]]") || label.ends_with(']') {
        return diag;
    }
    let pipe = if m.in_table { "\\|" } else { "|" };
    let fixed = if label.is_empty() || label == target { wiki } else { format!("{bang}[[{target}{pipe}{label}]]") };
    diag.with_fix(Fix::new(Applicability::Safe, format!("write {fixed}")).edit(&note.path, m.span.clone(), fixed))
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
                Dangling::Ambiguous(paths) => {
                    let paths: Vec<&str> = paths.iter().map(VaultPath::as_str).collect();
                    diag.with_hint(format!(
                        "several concepts are named `{target}` ({}); link by path",
                        paths.join(", ")
                    ))
                }
                Dangling::NoConcept { closest: Some(slug) } => diag
                    .with_hint(format!("no concept is named `{target}`; did you mean [[{slug}]]?"))
                    .with_fix(relink_fix(note, link, &slug)),
                Dangling::NoConcept { closest: None } => diag.with_hint(format!(
                    "no concept is named `{target}`; create it with `loci new`, or link a non-concept by path"
                )),
                Dangling::NoFile => diag,
            }
        }

        LinkError::Fragment { note: target_note, fragment: Fragment::Heading(heading) } => {
            at(Code::LinkDanglingHeading, format!("`{target}` has no heading `{heading}`"))
                .with_hint(headings_hint(target_note))
        }

        LinkError::Fragment { note: target_note, fragment: Fragment::Block(id) } => {
            at(Code::LinkDanglingBlock, format!("`{target}` has no block `^{id}`"))
                .with_hint(block_hint(target_note, &id))
        }
    };

    Some(diag)
}

/// The hint for a link to a missing block `^id` of `note`: an id with other characters
/// (`^step-1`) can end a line yet mark no block (§4.6), and then the hint says so.
fn block_hint(note: &Note, id: &str) -> String {
    let needle = format!("^{id}");
    let ends_line = |l: &str| {
        l.trim_end().strip_suffix(&needle).is_some_and(|rest| rest.is_empty() || rest.ends_with(char::is_whitespace))
    };
    if !crate::parse::is_block_id(id) && note.text.lines().any(ends_line) {
        format!(
            "`^{id}` is in `{}` but is no block id: ids are letters, digits and `_`; rename it there and here",
            note.path
        )
    } else {
        "the block was removed or its id changed; link the note, or another block by the `^id` that ends it".to_string()
    }
}

/// What `link` names, for messages: its slug or path, or the linking note's own name.
fn target_name<'a>(from: &'a Note, link: &'a Spanned<Link>) -> &'a str {
    link.target.names.name().unwrap_or_else(|| from.path.stem())
}
