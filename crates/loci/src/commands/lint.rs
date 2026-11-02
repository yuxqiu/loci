//! `loci lint` (m0 §9.2).

use loci_core::envelope::{Diagnostic, Refusal, Report, sort_diagnostics};
use loci_core::index::Index;
use loci_core::path::VaultPath;
use loci_core::plugin;
use loci_core::vault::Vault;
use loci_core::{fix, git, lint, relink};
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
    let mut diagnostics = lint::run(&index, &vault.config);
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

/// The `--fix` options.
pub struct FixArgs {
    pub unsafe_fixes: bool,
    pub only: Option<String>,
    pub allow_dirty: bool,
    pub dry_run: bool,
}

pub fn run(ctx: &Ctx, fix: Option<FixArgs>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let Some(args) = fix else {
        let linted = lint_vault(&vault);
        let (data, human) = (linted.data(), linted.summary());
        return Ok(Report::ok(data, human).with_diagnostics(linted.diagnostics));
    };
    if let Some(code) = &args.only {
        let update =
            code.strip_prefix("update.").map(|id| super::updates::all().iter().any(|u| id.parse() == Ok(u.id)));
        if !lint::known_code(code) || update == Some(false) {
            return Err(Refusal::refused("lint.unknown-code", format!("`{code}` is not a code core lint reports")));
        }
    }
    // As `cargo fix`: on a clean tree the diff is exactly the fixes. A dry run writes
    // nothing, so it may run on a dirty one.
    if !args.allow_dirty && !args.dry_run {
        let changes = git::status(&vault.root).map_err(Refusal::git)?;
        let dirty = [&changes.modified, &changes.deleted, &changes.added, &changes.untracked];
        if dirty.iter().any(|c| !c.is_empty()) {
            return Err(Refusal::refused(
                "lint.dirty",
                "the working tree has uncommitted changes; commit or stash them so the diff is the fixes', or pass `--allow-dirty`",
            ));
        }
    }
    let index = Index::open(&vault);
    let opts = fix::Options { unsafe_fixes: args.unsafe_fixes, only: args.only.as_deref() };
    let ignored = |p: &VaultPath| git::is_ignored(&vault.root, p).map_err(Refusal::git);
    let exists = |p: &VaultPath| vault.abs(p).symlink_metadata().is_ok();
    let plan = fix::plan(&index, &vault.config, &opts, ignored, exists)?;
    let mut human: String = plan
        .applied
        .iter()
        .map(|a| format!("fixed {}[{}]: {}\n", a.path.as_deref().unwrap_or("vault"), a.code, a.message))
        .collect();
    let linted = if args.dry_run {
        human.push_str(&super::plan_lines(&plan.change));
        // The core index rules after the fixes; nothing was written to run the rest on.
        Linted { files: index.files.len(), plugins: vec![], diagnostics: plan.remaining }
    } else {
        relink::apply(&vault, &plan.change).map_err(Refusal::io)?;
        lint_vault(&ctx.vault()?)
    };
    let mut data = linted.data();
    data["fixed"] = json!(plan.applied);
    data["dry_run"] = json!(args.dry_run);
    let verb = if args.dry_run { "would apply" } else { "applied" };
    human.push_str(&format!("{verb} {} fix(es); {}", plan.applied.len(), linted.summary()));
    Ok(Report::ok(data, human).with_diagnostics(linted.diagnostics))
}
