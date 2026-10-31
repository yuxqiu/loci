//! `loci lint` (m0 §9.2).

use loci_core::envelope::{Outcome, Report, sort_diagnostics};
use loci_core::index::Index;
use loci_core::lint::Lint;
use loci_core::plugin;
use serde_json::json;

use super::{Ctx, plugins};
use crate::VERSION;

pub fn run(ctx: &Ctx) -> Result<Report, Report> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let mut diags = Lint { index: &index, budgets: &vault.config.budgets, root: Some(&vault.root) }.run();
    let installed = plugins();
    for p in &installed {
        match plugin::lint(p, &vault.root, VERSION) {
            Ok(d) => diags.extend(d),
            Err(reason) => diags.push(plugin::failed(p, "lint", &reason)),
        }
    }
    sort_diagnostics(&mut diags);
    let errors = diags.iter().filter(|d| d.is_error()).count();
    let warnings = diags.len() - errors;
    let data = json!({
        "files": index.files.len(),
        "errors": errors,
        "warnings": warnings,
        "plugins": installed.iter().map(|p| &p.name).collect::<Vec<_>>(),
    });
    let outcome = Outcome::lint(data, diags);
    let human = format!("{} files, {errors} errors, {warnings} warnings\n", index.files.len());
    Ok(Report::new(outcome, human))
}
