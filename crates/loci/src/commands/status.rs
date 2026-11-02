//! `loci status` (m0 §9.6): what has changed in the vault. The rules that do not
//! change from task to task are read at session start (`AGENTS.md`), not here.

use std::collections::BTreeMap;

use loci_core::envelope::{Diagnostic, Refusal, Report};
use loci_core::git;
use loci_core::path::VaultPath;
use loci_core::vault::Kind;
use serde_json::json;

use super::Ctx;

/// Uncommitted inbox files listed by name in the human form; the rest are counted.
const LISTED: usize = 10;

pub fn run(ctx: &Ctx) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let root = &vault.root;
    let files = &vault.listing().files;
    let mut diags: Vec<Diagnostic> = vec![];
    let mut human = String::new();

    // 1. The agent entry point.
    if !files.iter().any(|f| f.as_str() == loci_core::vault::AGENTS) {
        diags.push(
            Diagnostic::warning("install.missing", "`AGENTS.md` is missing")
                .with_hint("run `loci init` in a new vault"),
        );
    }

    // 2. Uncommitted changes. A git failure is reported and the other parts go on.
    let changes = match git::status(root) {
        Ok(c) => {
            human.push_str(&format!(
                "## Uncommitted changes\n\n{} modified, {} deleted, {} added, {} untracked\n",
                c.modified.len(),
                c.deleted.len(),
                c.added.len(),
                c.untracked.len()
            ));
            Some(c)
        }
        Err(e) => {
            human.push_str("## Uncommitted changes\n\nunknown: `git status` failed\n");
            diags.push(Diagnostic::warning("status.git", format!("`git status` failed: {e}")));
            None
        }
    };

    // 3. Agent work recorded by `loci done`, waiting for the `improve` skill.
    let records = match git::agent_refs(root) {
        Ok(r) => {
            human.push_str(&format!("\n## Agent work to review\n\n{} task(s) recorded by `loci done`\n", r.len()));
            Some(r.len())
        }
        Err(e) => {
            human.push_str("\n## Agent work to review\n\nunknown: git failed\n");
            diags.push(Diagnostic::warning("status.git", format!("listing agent records failed: {e}")));
            None
        }
    };

    // 4. Inbox: counts by folder, and the files not yet committed, which block work.
    let inbox: Vec<&VaultPath> = files.iter().filter(|f| f.kind() == Some(Kind::Inbox)).collect();
    let mut by_folder: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &inbox {
        let parts: Vec<&str> = f.components().collect();
        *by_folder.entry(if parts.len() > 2 { parts[1] } else { "" }).or_default() += 1;
    }
    let uncommitted: Option<Vec<&VaultPath>> = changes.as_ref().map(|c| {
        let changed = |f: &VaultPath| c.untracked.contains(f) || c.modified.contains(f) || c.added.contains(f);
        inbox.iter().copied().filter(|f| changed(f)).collect()
    });
    let folders: Vec<String> = by_folder
        .iter()
        .map(|(d, n)| if d.is_empty() { format!("{n} directly in inbox/") } else { format!("{d} {n}") })
        .collect();
    let uncommitted_count = uncommitted.as_ref().map_or("unknown".to_string(), |u| u.len().to_string());
    human.push_str(&format!("\n## Inbox\n\n{} file(s), {uncommitted_count} uncommitted", inbox.len()));
    if !folders.is_empty() {
        human.push_str(&format!(" ({})", folders.join(", ")));
    }
    human.push('\n');
    if let Some(u) = uncommitted.as_ref().filter(|u| !u.is_empty()) {
        for f in u.iter().take(LISTED) {
            human.push_str(&format!("- {f}\n"));
        }
        if u.len() > LISTED {
            human.push_str(&format!("- and {} more\n", u.len() - LISTED));
        }
        human.push_str("Next: ask the user to commit these before processing the inbox.\n");
        diags.push(Diagnostic::warning(
            "inbox.uncommitted",
            format!(
                "{} inbox file(s) uncommitted: commit them before processing; a dissolved draft must be in git",
                u.len()
            ),
        ));
    }

    if !inbox.is_empty() && uncommitted.as_ref().is_some_and(|u| u.is_empty()) {
        human.push_str("Next: process them with the intake skill (`loci skill show intake`).\n");
    }

    // 5. Vault updates waiting for the user's decision (`loci updates`).
    let list = super::updates::all();
    let waiting: Vec<_> = match super::updates::waiting(vault.config.updates, &list) {
        Ok(waiting) => waiting.iter().map(|u| u.id).collect(),
        Err(newer) => {
            diags.push(Diagnostic::warning(&newer.error.code, newer.error.message));
            vec![]
        }
    };
    if !waiting.is_empty() {
        let ids: Vec<String> = waiting.iter().map(ToString::to_string).collect();
        human.push_str(&format!(
            "\n## Updates\n\n{} waiting ({}); read them with `loci updates`\n",
            ids.len(),
            ids.join(", ")
        ));
    }

    let data = json!({
        "changes": changes.as_ref().map(|c| json!({
            "modified": c.modified.len(),
            "deleted": c.deleted.len(),
            "added": c.added.len(),
            "untracked": c.untracked.len(),
        })),
        "agent_records": records,
        "inbox": {"files": inbox.len(), "by_folder": by_folder, "uncommitted": uncommitted},
        "updates": waiting,
    });
    Ok(Report::ok(data, human).with_diagnostics(diags))
}
