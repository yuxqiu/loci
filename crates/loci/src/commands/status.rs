//! `loci status` (m0 §9.6).

use loci_core::digest;
use loci_core::envelope::{Diagnostic, Outcome, Report};
use loci_core::git;
use loci_core::index::Index;
use serde_json::json;

use super::{Ctx, plugins};
use crate::assets;

pub fn run(ctx: &Ctx) -> Result<Report, Report> {
    let vault = ctx.vault()?;
    let root = &vault.root;
    let mut diags: Vec<Diagnostic> = vec![];
    let mut human = String::new();

    // 1. The agent entry point.
    if !root.join("AGENTS.md").is_file() {
        diags.push(
            Diagnostic::warning("install.missing", "`AGENTS.md` is missing")
                .with_hint("run `loci init` in a new vault"),
        );
    }

    // 2. Plugins.
    let installed = plugins();
    human.push_str("## Plugins\n\n");
    if installed.is_empty() {
        human.push_str("none\n");
    }
    for p in &installed {
        human.push_str(&format!("- {} ({})\n", p.name, p.path.display()));
    }

    // 3. Framework rules.
    human.push('\n');
    human.push_str(assets::RULES);

    // 4. Uncommitted changes. A git failure is reported and the other parts go on.
    let changes = match git::status(root) {
        Ok(c) => {
            human.push_str(&format!(
                "\n## Uncommitted changes\n\n{} modified, {} deleted, {} added, {} untracked\n",
                c.modified.len(),
                c.deleted.len(),
                c.added.len(),
                c.untracked.len()
            ));
            Some(c)
        }
        Err(e) => {
            human.push_str("\n## Uncommitted changes\n\nunknown: `git status` failed\n");
            diags.push(Diagnostic::warning("status.git", format!("`git status` failed: {e}")));
            None
        }
    };

    // 5. Inbox.
    let index = Index::open(&vault);
    let inbox: Vec<&String> = index.files.iter().filter(|f| f.starts_with("inbox/")).collect();
    let uncommitted: Option<Vec<&String>> = changes.as_ref().map(|c| {
        let changed = |f: &String| c.untracked.contains(f) || c.modified.contains(f) || c.added.contains(f);
        inbox.iter().copied().filter(|f| changed(f)).collect()
    });
    for f in uncommitted.iter().flatten() {
        diags.push(
            Diagnostic::warning(
                "inbox.uncommitted",
                "commit these before processing; a dissolved draft must be in git",
            )
            .at_path(f.as_str()),
        );
    }
    let uncommitted_count = uncommitted.as_ref().map_or("unknown".to_string(), |u| u.len().to_string());
    human.push_str(&format!("\n## Inbox\n\n{} file(s), {uncommitted_count} uncommitted\n", inbox.len()));
    for f in &inbox {
        human.push_str(&format!("- {f}\n"));
    }

    // 6. Index.
    let areas = digest::areas(&index);
    let digest_text = digest::render_root(&areas);
    human.push('\n');
    human.push_str(&digest_text);

    // 7. Rule files.
    let mut rules = serde_json::Map::new();
    for name in ["RULES.md", "STYLE.md"] {
        human.push('\n');
        match std::fs::read_to_string(root.join(name)) {
            Ok(text) => {
                human.push_str(&text);
                rules.insert(name.to_string(), json!(text));
            }
            Err(e) => {
                human.push_str(&format!("`{name}`: cannot be read\n"));
                diags.push(Diagnostic::warning("status.rules", format!("`{name}` cannot be read: {e}")).at_path(name));
                rules.insert(name.to_string(), serde_json::Value::Null);
            }
        }
    }

    let data = json!({
        "plugins": installed,
        "changes": changes.as_ref().map(|c| json!({
            "modified": c.modified.len(),
            "deleted": c.deleted.len(),
            "added": c.added.len(),
            "untracked": c.untracked.len(),
        })),
        "inbox": {"files": inbox, "uncommitted": uncommitted},
        "digest": digest_text,
        "rules": rules,
    });
    Ok(Report::new(Outcome::ok(data).with_diagnostics(diags), human))
}
