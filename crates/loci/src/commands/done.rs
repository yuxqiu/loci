//! `loci done` (m1-m4 §6): the end of an agent task. Lints, then records the agent's
//! work as a local git ref for the `improve` skill.

use std::time::{SystemTime, UNIX_EPOCH};

use loci_core::envelope::{Diagnostic, Outcome, Refusal, Report, sort_diagnostics};
use loci_core::git;

use super::Ctx;
use super::lint::lint_vault;

/// Records older than this are deleted.
const KEEP_SECS: u64 = 90 * 24 * 60 * 60;

pub fn run(ctx: &Ctx) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let mut linted = lint_vault(&vault);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).expect("the clock is after 1970").as_secs();
    let record = git::record_agent_tree(&vault.root, now).unwrap_or_else(|e| {
        linted.diagnostics.push(Diagnostic::warning("done.record", format!("the agent's work was not recorded: {e}")));
        None
    });
    if let Err(e) = git::prune_agent_refs(&vault.root, now, KEEP_SECS) {
        linted.diagnostics.push(Diagnostic::warning("done.prune", format!("old records were not deleted: {e}")));
    }
    sort_diagnostics(&mut linted.diagnostics);
    let mut data = linted.data();
    data["record"] = record.clone().into();
    let human = match &record {
        Some(r) => format!("{}recorded {r}\n", linted.summary()),
        None => format!("{}nothing recorded\n", linted.summary()),
    };
    Ok(Report::new(Outcome::lint(data, linted.diagnostics), human))
}
