//! `loci merge` (m1-m4 §1).

use loci_core::envelope::{Refusal, Report};
use loci_core::index::Index;
use loci_core::merge;
use loci_core::path::Kind;
use serde_json::json;

use super::Ctx;

pub fn run(ctx: &Ctx, from: &str, into: &str, dry_run: bool) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let (from, into) = (index.note_named(from)?, index.note_named(into)?);
    if let Some(n) = [from, into].into_iter().find(|n| n.kind != Kind::Concept) {
        return Err(Refusal::refused("note.kind", format!("`{}` is not a concept", n.path)));
    }
    let change = merge::plan(&index, from, into)?;
    let (from, into) = (&from.path, &into.path);
    if !dry_run {
        change.apply(&vault)?;
    }

    let data = json!({
        "from": from,
        "into": into,
        "edited": super::edited(&change),
        "dry_run": dry_run,
    });

    let n = change.writes.len();
    let human = if dry_run {
        format!("would merge {from} into {into}; edit {n} file(s)\n{}", super::plan_lines(&change))
    } else {
        format!("merged {from} into {into}; edited {n} file(s)\n")
    };

    Ok(Report::ok(data, human))
}
