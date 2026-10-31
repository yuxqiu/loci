//! `loci done` (m1-m4 §6): the end of an agent task. Lints, then records the agent's
//! work as a local git ref for the `improve` skill.

use std::time::{SystemTime, UNIX_EPOCH};

use loci_core::envelope::{Diagnostic, Report, sort_diagnostics};
use loci_core::git;
use serde_json::json;

use super::{Ctx, lint};

/// Records older than this are deleted.
const KEEP_SECS: u64 = 90 * 24 * 60 * 60;

pub fn run(ctx: &Ctx) -> Result<Report, Report> {
    let mut report = lint::run(ctx)?;
    let vault = ctx.vault()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).expect("the clock is after 1970").as_secs();
    let recorded = git::record_agent_tree(&vault.root, now)
        .and_then(|r| git::prune_agent_refs(&vault.root, now, KEEP_SECS).map(|_| r));
    let record = match recorded {
        Ok(r) => r,
        Err(e) => {
            let d = Diagnostic::warning("done.record", format!("the agent's work was not recorded: {e}"));
            report.outcome.diagnostics.push(d);
            sort_diagnostics(&mut report.outcome.diagnostics);
            None
        }
    };
    if let Ok(data) = &mut report.outcome.data {
        data["record"] = json!(record);
    }
    report.human.push_str(&match &record {
        Some(r) => format!("recorded {r}\n"),
        None => "nothing recorded\n".to_string(),
    });
    Ok(report)
}
