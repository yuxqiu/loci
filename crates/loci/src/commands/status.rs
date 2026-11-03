//! `loci status` (m0 §9.6): what has changed in the vault. The rules that do not
//! change from task to task are read at session start (`AGENTS.md`), not here.

use std::collections::BTreeMap;

use loci_core::config::UpdateId;
use loci_core::envelope::{Diagnostic, Refusal, Report};
use loci_core::git;
use loci_core::path::{InboxPlace, VaultPath};
use serde_json::json;

use super::Ctx;

/// Uncommitted inbox files listed by name in the human form; the rest are counted.
const LISTED: usize = 10;

pub fn run(ctx: &Ctx) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let files = &vault.listing().files;
    let mut out = Out::default();

    entry_point(files, &mut out);
    let changes = changes(vault.repo(), &mut out);
    let records = agent_work(vault.repo(), &mut out);
    let inbox = inbox(files, changes.as_ref(), &mut out);
    let updates = updates(vault.config.updates, &mut out);

    let data = json!({
        "changes": changes.as_ref().map(|c| json!({
            "modified": c.modified.len(),
            "deleted": c.deleted.len(),
            "added": c.added.len(),
            "untracked": c.untracked.len(),
        })),
        "agent_records": records,
        "inbox": inbox,
        "updates": updates,
    });

    Ok(Report::ok(data, out.human).with_diagnostics(out.diags))
}

/// What the sections write: the human text and the warnings.
#[derive(Default)]
struct Out {
    human: String,
    diags: Vec<Diagnostic>,
}

/// The agent entry point, `AGENTS.md`.
fn entry_point(files: &[VaultPath], out: &mut Out) {
    if !files.iter().any(|f| *f == loci_core::path::AGENTS) {
        let warning = Diagnostic::warning("install.missing", "`AGENTS.md` is missing");
        out.diags.push(warning.with_hint("run `loci init` in a new vault"));
    }
}

/// Uncommitted changes, `None` when git failed (reported; the other sections go on).
fn changes(repo: git::Repo, out: &mut Out) -> Option<git::Changes> {
    out.human.push_str("## Uncommitted changes\n\n");
    match repo.status() {
        Ok(c) => {
            let (m, d, a, u) = (c.modified.len(), c.deleted.len(), c.added.len(), c.untracked.len());
            out.human.push_str(&format!("{m} modified, {d} deleted, {a} added, {u} untracked\n"));
            Some(c)
        }
        Err(e) => {
            out.human.push_str("unknown: `git status` failed\n");
            out.diags.push(Diagnostic::warning("status.git", format!("`git status` failed: {e}")));
            None
        }
    }
}

/// Agent work recorded by `loci done`, waiting for the `improve` skill: how many records.
fn agent_work(repo: git::Repo, out: &mut Out) -> Option<usize> {
    out.human.push_str("\n## Agent work to review\n\n");
    match repo.agent_refs() {
        Ok(r) => {
            out.human.push_str(&format!("{} task(s) recorded by `loci done`\n", r.len()));
            Some(r.len())
        }
        Err(e) => {
            out.human.push_str("unknown: git failed\n");
            out.diags.push(Diagnostic::warning("status.git", format!("listing agent records failed: {e}")));
            None
        }
    }
}

/// The inbox: counts by folder, and the files not yet committed, which block work (at
/// most [`LISTED`] named), then the next step.
fn inbox(files: &[VaultPath], changes: Option<&git::Changes>, out: &mut Out) -> serde_json::Value {
    let inbox: Vec<&VaultPath> = files.iter().filter(|f| f.inbox_place().is_some()).collect();
    let mut by_place: BTreeMap<InboxPlace, usize> = BTreeMap::new();
    for place in inbox.iter().filter_map(|f| f.inbox_place()) {
        *by_place.entry(place).or_default() += 1;
    }

    let uncommitted: Option<Vec<&VaultPath>> = changes.map(|c| {
        let changed = |f: &VaultPath| c.untracked.contains(f) || c.modified.contains(f) || c.added.contains(f);
        inbox.iter().copied().filter(|f| changed(f)).collect()
    });

    let count = uncommitted.as_ref().map_or("unknown".to_string(), |u| u.len().to_string());
    out.human.push_str(&format!("\n## Inbox\n\n{} file(s), {count} uncommitted", inbox.len()));
    let folders: Vec<String> = by_place
        .iter()
        .map(|(place, n)| match place {
            InboxPlace::Root => format!("{n} directly in inbox/"),
            InboxPlace::Folder(d) => format!("{d} {n}"),
        })
        .collect();
    if !folders.is_empty() {
        out.human.push_str(&format!(" ({})", folders.join(", ")));
    }

    out.human.push('\n');

    match uncommitted.as_deref() {
        Some(u) if !u.is_empty() => {
            for f in u.iter().take(LISTED) {
                out.human.push_str(&format!("- {f}\n"));
            }
            if u.len() > LISTED {
                out.human.push_str(&format!("- and {} more\n", u.len() - LISTED));
            }
            out.human.push_str("Next: ask the user to commit these before processing the inbox.\n");
            let n = u.len();
            out.diags.push(Diagnostic::warning(
                "inbox.uncommitted",
                format!(
                    "{n} inbox file(s) uncommitted: commit them before processing; a dissolved draft must be in git"
                ),
            ));
        }
        Some(_) if !inbox.is_empty() => {
            out.human.push_str("Next: process them with the intake skill (`loci skill show intake`).\n");
        }
        _ => {}
    }

    // In JSON, files directly in `inbox/` are counted under `""` (m0 §9.6).
    let by_folder: BTreeMap<&str, usize> =
        by_place.iter().map(|(place, n)| (if let InboxPlace::Folder(d) = place { *d } else { "" }, *n)).collect();
    json!({"files": inbox.len(), "by_folder": by_folder, "uncommitted": uncommitted})
}

/// Vault updates waiting for the user's decision (`loci updates`): their ids, or a
/// warning when the vault is newer than this loci.
fn updates(mark: Option<UpdateId>, out: &mut Out) -> Vec<UpdateId> {
    let list = super::updates::all();
    let waiting: Vec<UpdateId> = match super::updates::waiting(mark, &list) {
        Ok(waiting) => waiting.iter().map(|u| u.id).collect(),
        Err(newer) => {
            out.diags.push(Diagnostic::warning(&newer.error.code, newer.error.message));
            vec![]
        }
    };
    if !waiting.is_empty() {
        let ids: Vec<String> = waiting.iter().map(ToString::to_string).collect();
        let line = format!("{} waiting ({}); read them with `loci updates`", ids.len(), ids.join(", "));
        out.human.push_str(&format!("\n## Updates\n\n{line}\n"));
    }
    waiting
}
