//! `loci done` (m1-m4 §6): the end of an agent task. Lints, then records the agent's
//! work as a local git ref for the `improve` skill.

use std::time::{Duration, SystemTime};

use loci_core::envelope::{Diagnostic, Refusal, Report, sort_diagnostics};

use super::Ctx;
use super::lint::lint_vault;

/// Records older than this are deleted.
const KEEP: Duration = Duration::from_secs(90 * 24 * 60 * 60);

pub fn run(ctx: &Ctx, notes: &[String]) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let mut linted = lint_vault(&vault);
    let now = SystemTime::now();
    let record = vault.repo().record_agent_tree(now, notes).unwrap_or_else(|e| {
        linted.diagnostics.push(Diagnostic::warning("done.record", format!("the agent's work was not recorded: {e}")));
        None
    });
    if let Err(e) = vault.repo().prune_agent_refs(now, KEEP) {
        linted.diagnostics.push(Diagnostic::warning("done.prune", format!("old records were not deleted: {e}")));
    }
    sort_diagnostics(&mut linted.diagnostics);
    let mut data = linted.data();
    data["record"] = record.clone().into();
    let human = match &record {
        Some(r) => format!("{}recorded {r}\n", linted.summary()),
        None => format!("{}nothing recorded\n", linted.summary()),
    };
    Ok(Report::ok(data, human).with_diagnostics(linted.diagnostics))
}
