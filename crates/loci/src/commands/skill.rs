//! `loci skill show` (m0 §9.8).

use loci_core::envelope::{Diagnostic, Refusal, Report};
use loci_core::plugin::Skill;
use serde_json::json;

use super::{Ctx, plugins};
use crate::{VERSION, assets};

/// Core skills and every installed plugin's skills, with the owning plugin. Plugins
/// whose `skills` hook fails become diagnostics.
fn all_skills(vault_root: &std::path::Path) -> (Vec<(Skill, Option<String>)>, Vec<Diagnostic>) {
    let mut out: Vec<(Skill, Option<String>)> = assets::skills().into_iter().map(|s| (s, None)).collect();
    let mut diags = vec![];
    for p in plugins() {
        match p.skills(vault_root, VERSION) {
            Ok(skills) => out.extend(skills.into_iter().map(|s| (s, Some(p.name.clone())))),
            Err(failed) => diags.push(*failed),
        }
    }
    (out, diags)
}

pub fn run(ctx: &Ctx, name: Option<&str>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let (skills, diags) = all_skills(vault.root());
    match name {
        None => {
            let human: String = skills.iter().map(|(s, _)| format!("{s}\n")).collect();
            let data = json!({"skills": skills.iter().map(|(s, p)| json!({
                "name": s.name, "description": s.description, "plugin": p,
            })).collect::<Vec<_>>()});
            Ok(Report::ok(data, human).with_diagnostics(diags))
        }
        Some(name) => match skills.iter().find(|(s, _)| s.name == name) {
            Some((s, p)) => {
                let data = json!({"name": s.name, "description": s.description, "body": s.body, "plugin": p});
                Ok(Report::ok(data, s.body.clone()).with_diagnostics(diags))
            }
            None => {
                let names: Vec<&str> = skills.iter().map(|(s, _)| s.name.as_str()).collect();
                let msg = format!("no skill `{name}`; skills: {}", names.join(", "));
                // A report, not a bare refusal, so the diagnostics are kept.
                Ok(Report::from(Refusal::refused("skill.unknown", msg)).with_diagnostics(diags))
            }
        },
    }
}
