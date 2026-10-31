//! `loci new` (m0 §9.8).

use loci_core::envelope::{Outcome, Report};
use loci_core::index::Index;
use loci_core::vault::is_slug;
use loci_core::vault::{self, Kind};
use serde_json::json;

use super::{Ctx, vault_path};

pub fn run(ctx: &Ctx, path: &str, title: Option<&str>) -> Result<Report, Report> {
    let vault = ctx.vault()?;
    let mut path = vault_path(path)?;
    if !path.ends_with(".md") {
        path.push_str(".md");
    }
    if vault::kind_of(&path) != Some(Kind::Concept) {
        return Err(Report::fail(Outcome::refused("new.no-area", format!("`{path}` is not inside an area"))));
    }
    let slug = vault::stem(&path).to_string();
    if !is_slug(&slug) {
        return Err(Report::fail(Outcome::refused(
            "slug.format",
            format!("slug `{slug}` does not match the slug pattern"),
        )));
    }
    let abs = vault.abs(&path);
    if abs.exists() {
        return Err(Report::fail(Outcome::refused("new.exists", format!("`{path}` already exists"))));
    }
    let index = Index::open(&vault);
    let key = slug.to_lowercase();
    if index.slugs.contains_key(&key) || index.aliases.contains_key(&key) {
        return Err(Report::fail(Outcome::refused("new.collision", format!("`{slug}` is already a slug or alias"))));
    }
    let id = vault::new_concept_id();
    let text = format!(
        "---\nid: {id}\naliases: []\nprereqs: []\nsources: []\ndigest: \"\"\n---\n# {}\n",
        title.unwrap_or(&slug)
    );
    if let Some(parent) = abs.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return Err(Report::fail(Outcome::internal("io", e.to_string())));
    }
    if let Err(e) = std::fs::write(&abs, text) {
        return Err(Report::fail(Outcome::internal("io", e.to_string())));
    }
    Ok(Report::new(Outcome::ok(json!({"path": path, "id": id})), format!("{path} {id}\n")))
}
