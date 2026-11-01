//! `loci lint` (m0 §9.2).

use loci_core::envelope::{Diagnostic, Outcome, Refusal, Report, sort_diagnostics};
use loci_core::index::Index;
use loci_core::lint::{self, Lint};
use loci_core::plugin;
use loci_core::vault::Vault;
use serde_json::{Value, json};

use super::{Ctx, plugins};
use crate::VERSION;

/// The result of linting a vault: core rules and every installed plugin's lint.
pub struct Linted {
    pub files: usize,
    pub plugins: Vec<String>,
    /// Sorted (m0 §7.1).
    pub diagnostics: Vec<Diagnostic>,
}

impl Linted {
    /// The `data` of `loci lint` (m0 §9.2).
    pub fn data(&self) -> Value {
        let errors = self.diagnostics.iter().filter(|d| d.is_error()).count();
        json!({
            "files": self.files,
            "errors": errors,
            "warnings": self.diagnostics.len() - errors,
            "plugins": self.plugins,
        })
    }

    /// The human summary line.
    pub fn summary(&self) -> String {
        let errors = self.diagnostics.iter().filter(|d| d.is_error()).count();
        format!("{} files, {errors} errors, {} warnings\n", self.files, self.diagnostics.len() - errors)
    }
}

pub fn lint_vault(vault: &Vault) -> Linted {
    let index = Index::open(vault);
    let mut diagnostics = Lint { index: &index, budgets: &vault.config.budgets }.run();
    diagnostics.extend(lint::disk_rules(&index, &vault.root));
    let installed = plugins();
    for p in &installed {
        match plugin::lint(p, &vault.root, VERSION) {
            Ok(d) => diagnostics.extend(d),
            Err(reason) => diagnostics.push(plugin::failed(p, "lint", &reason)),
        }
    }
    sort_diagnostics(&mut diagnostics);
    Linted { files: index.files.len(), plugins: installed.into_iter().map(|p| p.name).collect(), diagnostics }
}

pub fn run(ctx: &Ctx) -> Result<Report, Refusal> {
    let linted = lint_vault(&ctx.vault()?);
    let (data, human) = (linted.data(), linted.summary());
    Ok(Report::new(Outcome::lint(data, linted.diagnostics), human))
}
