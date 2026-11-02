//! `loci new` (m0 §9.7).

use loci_core::envelope::{Diagnostic, Refusal, Report};
use loci_core::git;
use loci_core::index::Index;
use loci_core::path::VaultPath;
use loci_core::vault::{self, Kind};
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, path: &str, title: Option<&str>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let mut path = VaultPath::from_arg(path)?;
    if !path.as_str().ends_with(".md") {
        path = path.with_suffix(".md");
    }
    if path.kind() != Some(Kind::Concept) {
        return Err(Refusal::refused("new.no-area", format!("`{path}` {}", path.why_not_concept())));
    }
    if let Some((code, message)) = path.problem() {
        return Err(Refusal::refused(code, format!("`{path}`: {message}")));
    }
    let slug = path.stem().to_string();
    let abs = vault.abs(&path);
    if abs.symlink_metadata().is_ok() {
        return Err(Refusal::refused("new.exists", format!("`{path}` already exists")));
    }
    // The destination checks `mv` makes too (m0 §9.5).
    let index = Index::open(&vault);
    if let Some((existing, asked)) = index.case_twin(&path) {
        return Err(Refusal::refused(
            "path.case-twin",
            format!(
                "`{asked}` differs only in letter case from the existing `{existing}`; use the existing spelling or another name"
            ),
        ));
    }
    if git::is_ignored(&vault.root, &path).map_err(Refusal::git)? {
        return Err(Refusal::refused("path.ignored", format!("`{path}` is ignored by git")));
    }
    if index.name_taken(&slug, |_| false) {
        let existing: Vec<String> = index.slug_paths(&slug).iter().map(|p| format!("`{p}`")).collect();
        let existing = existing.join(", ");
        return Err(Refusal::refused("new.collision", format!("`{slug}` is already the slug of {existing}")));
    }
    let id = vault::new_concept_id();
    let text = format!("---\nid: {id}\nprereqs: []\n---\n# {}\n", title.unwrap_or(&slug));
    if let Some(parent) = abs.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return Err(Refusal::io(e));
    }
    std::fs::write(&abs, text).map_err(Refusal::io)?;
    // Marked for `git diff`, as the framework rules ask of every file an agent creates.
    let mut diags = vec![];
    if let Err(e) = git::run(&vault.root, &["add", "-N", "--", path.as_str()]) {
        diags.push(
            Diagnostic::warning("new.git-add", format!("created, but `git add -N` failed: {e}; run it yourself"))
                .at_path(path.as_str()),
        );
    }
    Ok(Report::ok(json!({"path": path, "id": id}), format!("{path} {id}\n")).with_diagnostics(diags))
}
