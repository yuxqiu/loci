//! `loci lint` (m0 §9.2).

use loci_core::envelope::{Diagnostic, Refusal, Report, sort_diagnostics};
use loci_core::index::Index;
use loci_core::path::VaultPath;
use loci_core::vault::Vault;
use loci_core::{fix, lint};
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
    /// Lints the vault: core rules, disk rules and every installed plugin's.
    pub fn of(vault: &Vault) -> Self {
        let index = Index::open(vault);
        let mut diagnostics = lint::run(&index);
        diagnostics.extend(lint::disk_rules(&index, vault.repo()));
        let installed = plugins();
        for p in &installed {
            match p.lint(vault.root(), VERSION) {
                Ok(d) => diagnostics.extend(d),
                Err(failed) => diagnostics.push(*failed),
            }
        }
        sort_diagnostics(&mut diagnostics);
        Linted { files: vault.files().len(), plugins: installed.into_iter().map(|p| p.name).collect(), diagnostics }
    }

    /// The `data` of `loci lint` (m0 §9.2).
    pub fn data(&self) -> Value {
        json!({
            "files": self.files,
            "errors": self.errors(),
            "warnings": self.diagnostics.len() - self.errors(),
            "plugins": self.plugins,
        })
    }

    /// The human summary line.
    pub fn summary(&self) -> String {
        let errors = self.errors();
        format!("{} files, {errors} errors, {} warnings\n", self.files, self.diagnostics.len() - errors)
    }

    fn errors(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.is_error()).count()
    }
}

/// `loci lint`'s fix options.
#[derive(clap::Args)]
pub struct FixArgs {
    /// Apply the safe fixes of core diagnostics, then lint again.
    #[arg(long)]
    fix: bool,
    /// Apply unsafe fixes too (likely, but they may change meaning).
    #[arg(long, requires = "fix")]
    unsafe_fixes: bool,
    /// Only fix diagnostics with this code.
    #[arg(long, requires = "fix", value_name = "CODE")]
    only: Option<String>,
    /// Fix on a working tree with uncommitted changes (refused otherwise, so the diff is
    /// the fixes').
    #[arg(long, requires = "fix")]
    allow_dirty: bool,
    /// Report the fixes; write nothing.
    #[arg(long, requires = "fix")]
    dry_run: bool,
}

pub fn run(ctx: &Ctx, args: &FixArgs) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    if !args.fix {
        let linted = Linted::of(&vault);
        let (data, human) = (linted.data(), linted.summary());
        return Ok(Report::ok(data, human).with_diagnostics(linted.diagnostics));
    };
    let only = fix_preconditions(&vault, args)?;
    fix_vault(ctx, &vault, args, only)
}

/// The code `--only` names, after refusing one core lint does not report
/// (`lint.unknown-code`) and, as `cargo fix` does, a working tree with uncommitted
/// changes (`lint.dirty`): on a clean tree the diff is exactly the fixes. A dry run
/// writes nothing, so it may run on a dirty one.
fn fix_preconditions(vault: &Vault, args: &FixArgs) -> Result<Option<lint::Code>, Refusal> {
    let unknown =
        |code: &str| Refusal::refused("lint.unknown-code", format!("`{code}` is not a code core lint reports"));
    let only = args.only.as_deref().map(|c| lint::core_code(c).ok_or_else(|| unknown(c))).transpose()?;
    if !args.allow_dirty && !args.dry_run && !vault.repo().status().map_err(Refusal::git)?.is_clean() {
        return Err(Refusal::refused(
            "lint.dirty",
            "the working tree has uncommitted changes; commit or stash them so the diff is the fixes', or pass `--allow-dirty`",
        ));
    }
    Ok(only)
}

/// `--fix`: plans the fixes, writes them unless it is a dry run, and lints again.
fn fix_vault(ctx: &Ctx, vault: &Vault, args: &FixArgs, only: Option<lint::Code>) -> Result<Report, Refusal> {
    let index = Index::open(vault);
    let opts = fix::Options { unsafe_fixes: args.unsafe_fixes, only };
    let plan = fix::Plan::new(&index, &opts)?;
    let mut human: String = plan
        .applied
        .iter()
        .map(|a| format!("fixed {}[{}]: {}\n", a.path.as_ref().map_or("vault", VaultPath::as_str), a.code, a.message))
        .collect();
    let linted = if args.dry_run {
        human.push_str(&super::plan_lines(&plan.change));
        // The core index rules after the fixes; nothing was written to run the rest on.
        Linted { files: index.vault().files().len(), plugins: vec![], diagnostics: plan.remaining }
    } else {
        plan.change.apply(vault)?;
        Linted::of(&ctx.vault()?)
    };

    let mut data = linted.data();
    data["fixed"] = json!(plan.applied);
    data["dry_run"] = json!(args.dry_run);
    data["unapplied"] = json!(plan.unapplied);
    let verb = if args.dry_run { "would apply" } else { "applied" };
    human.push_str(&format!("{verb} {} fix(es); {}", plan.applied.len(), linted.summary()));
    if plan.unapplied > 0 {
        human.push_str(&format!(
            "\n{} fix(es) left after {} passes; run `loci lint --fix` again",
            plan.unapplied,
            fix::PASSES
        ));
    }
    Ok(Report::ok(data, human).with_diagnostics(linted.diagnostics))
}
