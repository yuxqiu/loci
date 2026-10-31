//! `loci init` (m0 §9.1).

use loci_core::config::{self, Config};
use loci_core::envelope::{Outcome, Report};
use loci_core::git;
use serde_json::json;

use super::Ctx;
use crate::assets;

const GITIGNORE_LINES: [&str; 3] = ["/.agents/", "/.claude/skills", "/.loci/"];

pub fn run(ctx: &Ctx) -> Result<Report, Report> {
    let dir = match &ctx.explicit_vault {
        Some(d) if d.is_absolute() => d.clone(),
        Some(d) => ctx.cwd.join(d),
        None => ctx.cwd.clone(),
    };
    if dir.join(config::FILE).exists() {
        return Err(Report::fail(Outcome::refused("init.exists", "loci.toml already exists")));
    }
    if !git::in_worktree(&dir) {
        if let Err(e) = git::run(&dir, &["init", "-q"]) {
            return Err(Report::fail(Outcome::internal("git", e)));
        }
    } else if !git::is_worktree_root(&dir) {
        return Err(Report::fail(Outcome::refused(
            "vault.not-git",
            "run `loci init` at the root of the git work tree",
        )));
    }
    let files: [(&str, String); 4] = [
        (config::FILE, Config::default_file()),
        ("RULES.md", "# Rules\n\n## Lessons\n".to_string()),
        ("STYLE.md", "# Style\n".to_string()),
        ("AGENTS.md", assets::AGENTS_STUB.to_string()),
    ];
    let (mut created, mut skipped) = (vec![], vec![]);
    for (name, content) in files {
        let path = dir.join(name);
        if path.exists() {
            skipped.push(name.to_string());
        } else if let Err(e) = std::fs::write(&path, content) {
            return Err(Report::fail(Outcome::internal("io", e.to_string())));
        } else {
            created.push(name.to_string());
        }
    }
    // Read as bytes and append, so a `.gitignore` that is not UTF-8 is kept intact.
    let gi = dir.join(".gitignore");
    let existing = match std::fs::read(&gi) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
        Err(e) => return Err(Report::fail(Outcome::internal("io", format!("reading `.gitignore`: {e}")))),
    };
    let has =
        |line: &str| existing.split(|b| *b == b'\n').any(|l| l.strip_suffix(b"\r").unwrap_or(l) == line.as_bytes());
    let missing: Vec<&str> = GITIGNORE_LINES.iter().copied().filter(|l| !has(l)).collect();
    if missing.is_empty() {
        skipped.push(".gitignore".to_string());
    } else {
        let mut bytes = existing;
        if !bytes.is_empty() && !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        for l in missing {
            bytes.extend_from_slice(l.as_bytes());
            bytes.push(b'\n');
        }
        if let Err(e) = std::fs::write(&gi, bytes) {
            return Err(Report::fail(Outcome::internal("io", e.to_string())));
        }
        created.push(".gitignore".to_string());
    }
    let human = format!("created: {}\nskipped: {}\n", created.join(", "), skipped.join(", "));
    Ok(Report::new(Outcome::ok(json!({"created": created, "skipped": skipped})), human))
}
