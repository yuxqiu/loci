//! `loci new` (m0 §9.7).

use std::io::Write;

use loci_core::envelope::{Diagnostic, Refusal, Report};
use loci_core::id::{Slug, Ulid};
use loci_core::index::Index;
use loci_core::path::Kind;
use loci_core::path::VaultPath;
use loci_core::vault::Vault;
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, path: &str, title: Option<&str>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let path = VaultPath::from_arg(path)?.to_note();
    let slug = refuse_destination(&vault, &path)?;

    let id = Ulid::generate();
    let text = skeleton(id, title.unwrap_or(slug.as_str()));
    let abs = vault.abs(&path);
    std::fs::create_dir_all(abs.parent().expect("a note path has a folder")).map_err(Refusal::io)?;
    // `create_new`: never through a symlink, nor over a file made since the check.
    std::fs::File::create_new(&abs).and_then(|mut f| f.write_all(text.as_bytes())).map_err(Refusal::io)?;

    // Marked for `git diff`, as the framework rules ask of every file an agent creates.
    let mut diags = vec![];
    if let Err(e) = vault.repo().intent_to_add(&path) {
        diags.push(
            Diagnostic::warning("new.git-add", format!("created, but `git add -N` failed: {e}; run it yourself"))
                .at_path(&path),
        );
    }

    Ok(Report::ok(json!({"path": path, "id": id}), format!("{path} {id}\n")).with_diagnostics(diags))
}

/// A new concept: its frontmatter, then its title.
fn skeleton(id: Ulid, title: &str) -> String {
    format!("{}# {title}\n", loci_core::parse::new_concept_frontmatter(id))
}

/// The slug of the concept `path` would be, after checking that it can be created there:
/// a concept path that follows the path rules, free on disk, and passing the destination
/// checks `mv` makes too (m0 §9.5): no case twin, not ignored, no slug in use.
fn refuse_destination(vault: &Vault, path: &VaultPath) -> Result<Slug, Refusal> {
    if path.kind() != Some(Kind::Concept) {
        return Err(Refusal::refused("new.no-area", format!("`{path}` {}", path.why_not_concept())));
    }

    if let Some(problem) = path.problem() {
        return Err(Refusal::refused(problem.code(), format!("`{path}`: {problem}")));
    }

    if vault.abs(path).symlink_metadata().is_ok() {
        return Err(Refusal::refused("new.exists", format!("`{path}` already exists")));
    }

    let index = Index::open(vault);
    loci_core::mv::refuse_case_twin(&index, path, None)?;

    if vault.repo().is_ignored(path).map_err(Refusal::git)? {
        return Err(Refusal::refused("path.ignored", format!("`{path}` is ignored by git")));
    }

    let slug = path.slug().expect("`problem` refused a concept whose stem is no slug");
    let existing: Vec<String> = index.concepts_named(slug.as_str()).iter().map(|p| format!("`{p}`")).collect();
    if !existing.is_empty() {
        let message = format!("`{slug}` is already the slug of {}", existing.join(", "));
        return Err(Refusal::refused("new.collision", message));
    }

    Ok(slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skeleton_is_canonical() {
        let text = skeleton(Ulid::generate(), "BFS");
        let note = loci_core::parse::parse(&VaultPath::new("concepts/a/bfs.md".into()).unwrap(), text.clone());
        let rewritten = loci_core::splice::apply(&text, vec![note.rewrite_frontmatter(&[])]);
        assert_eq!(rewritten.unwrap(), text, "a fix would not reformat a new concept");
    }
}
