//! `loci init` (m0 §9.1).

use loci_core::config::{self, Config};
use loci_core::envelope::{Refusal, Report};
use loci_core::git;
use serde_json::json;

use super::Ctx;
use crate::assets;

const GITIGNORE_LINES: [&str; 1] = ["/.loci/"];

pub fn run(ctx: &Ctx) -> Result<Report, Refusal> {
    let dir = match &ctx.explicit_vault {
        Some(d) if d.is_absolute() => d.clone(),
        Some(d) => ctx.cwd.join(d),
        None => ctx.cwd.clone(),
    };
    let present = |name: &str| dir.join(name).symlink_metadata().is_ok();
    if present(config::FILE) {
        return Err(Refusal::refused("init.exists", "loci.toml already exists"));
    }
    // There is no vault to check yet (`Vault::open` refuses symlinks): before anything
    // is written, the files `init` manages must not be links (m0 §3).
    let managed = ["RULES.md", "STYLE.md", "AGENTS.md", ".gitignore"];
    if let Some(name) = managed.into_iter().find(|n| dir.join(n).symlink_metadata().is_ok_and(|m| m.is_symlink())) {
        let message = format!("`{name}` is a symlink; loci does not follow symlinks");
        return Err(Refusal::refused("file.symlink", message));
    }
    match git::toplevel(&dir) {
        // Not in a work tree (or git cannot tell): `git init` creates one or fails loudly.
        Err(_) => {
            git::run(&dir, &["init", "-q"]).map_err(Refusal::git)?;
        }
        Ok(top) if dir.canonicalize().is_ok_and(|d| d == top) => {}
        Ok(_) => return Err(Refusal::refused("vault.not-git", "run `loci init` at the root of the git work tree")),
    }
    let files: [(&str, String); 4] = [
        (config::FILE, Config::default_file(&super::updates::all().last().expect("updates ship").id)),
        ("RULES.md", "# Rules\n\n## Lessons\n".to_string()),
        ("STYLE.md", "# Style\n".to_string()),
        ("AGENTS.md", assets::AGENTS_STUB.to_string()),
    ];
    let (mut created, mut skipped) = (vec![], vec![]);
    for (name, content) in files {
        if present(name) {
            skipped.push(name.to_string());
        } else {
            std::fs::write(dir.join(name), content).map_err(Refusal::io)?;
            created.push(name.to_string());
        }
    }
    // Read as bytes and append, so a `.gitignore` that is not UTF-8 is kept intact.
    let gi = dir.join(".gitignore");
    let existing = match std::fs::read(&gi) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
        Err(e) => return Err(Refusal::io(format!("reading `.gitignore`: {e}"))),
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
        std::fs::write(&gi, bytes).map_err(Refusal::io)?;
        created.push(".gitignore".to_string());
    }
    let human = format!("created: {}\nskipped: {}\n", created.join(", "), skipped.join(", "));
    Ok(Report::ok(json!({"created": created, "skipped": skipped}), human))
}
