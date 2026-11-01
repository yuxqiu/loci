//! `loci new` (m0 §9.7).

use loci_core::envelope::{Refusal, Report};
use loci_core::git;
use loci_core::index::Index;
use loci_core::vault::{self, Kind};
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, path: &str, title: Option<&str>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let mut path = vault::path_arg(path)?;
    if !path.ends_with(".md") {
        path.push_str(".md");
    }
    if vault::kind_of(&path) != Some(Kind::Concept) {
        return Err(Refusal::refused("new.no-area", format!("`{path}` {}", vault::why_not_concept(&path))));
    }
    if let Some((code, message)) = vault::path_problem(&path) {
        return Err(Refusal::refused(code, format!("`{path}`: {message}")));
    }
    let slug = vault::stem(&path).to_string();
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
        return Err(Refusal::refused("new.collision", format!("`{slug}` is already a concept's slug")));
    }
    let id = vault::new_concept_id();
    let text = format!("---\nid: {id}\nprereqs: []\ndigest: \"\"\n---\n# {}\n", title.unwrap_or(&slug));
    if let Some(parent) = abs.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return Err(Refusal::io(e));
    }
    std::fs::write(&abs, text).map_err(Refusal::io)?;
    Ok(Report::ok(json!({"path": path, "id": id}), format!("{path} {id}\n")))
}
