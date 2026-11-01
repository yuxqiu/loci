//! `loci digest` (m0 §9.4).

use loci_core::digest;
use loci_core::envelope::{Outcome, Refusal, Report};
use loci_core::index::Index;

use super::Ctx;

pub fn run(ctx: &Ctx, area: Option<&str>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let areas = digest::areas(&index);
    match area {
        None => Ok(Report::new(Outcome::ok(digest::root_json(&areas)), digest::render_root(&areas))),
        Some(name) => match areas.iter().find(|a| a.name == name) {
            Some(a) => Ok(Report::new(Outcome::ok(digest::area_json(a)), digest::render_area(a))),
            None => Err(Refusal::refused("digest.no-area", format!("there is no area `{name}`"))),
        },
    }
}
