//! `loci init` (m0 §9.1).

use loci_core::config::{self, Config};
use loci_core::envelope::{Refusal, Report};
use loci_core::{git, path};
use serde_json::json;

use super::Ctx;
use crate::assets;

/// The line `.gitignore` needs: exports are written under `.loci/`.
const GITIGNORE_LINE: &str = "/.loci/";

pub fn run(ctx: &Ctx) -> Result<Report, Refusal> {
    // `init` creates the vault where it is told, or right here: it never searches up.
    let dir = ctx.vault.dir().to_path_buf();

    let present = |name: &str| dir.join(name).symlink_metadata().is_ok();
    if present(config::FILE) {
        return Err(Refusal::refused("init.exists", "loci.toml already exists"));
    }

    // There is no vault to check yet (`Vault::open` refuses symlinks): before anything
    // is written, the files `init` manages must not be links (m0 §3).
    let managed = [path::RULES, path::STYLE, path::AGENTS, ".gitignore"];
    if let Some(name) = managed.into_iter().find(|n| dir.join(n).symlink_metadata().is_ok_and(|m| m.is_symlink())) {
        let message = format!("`{name}` is a symlink; loci does not follow symlinks");
        return Err(Refusal::refused("file.symlink", message));
    }

    match git::toplevel(&dir) {
        // Not in a work tree (or git cannot tell): `git init` creates one or fails loudly.
        Err(_) => {
            git::Repo::new(&dir).init().map_err(Refusal::git)?;
        }
        Ok(top) if dir.canonicalize().is_ok_and(|d| d == top) => {}
        Ok(_) => return Err(Refusal::refused("vault.not-git", "run `loci init` at the root of the git work tree")),
    }

    let files: [(&str, String); 4] = [
        (config::FILE, Config::default_file(super::updates::all().last().expect("updates ship").id)),
        (path::RULES, "# Rules\n\n## Lessons\n".to_string()),
        (path::STYLE, "# Style\n".to_string()),
        (path::AGENTS, assets::AGENTS_STUB.to_string()),
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

    if add_gitignore_line(&dir.join(".gitignore"))? {
        created.push(".gitignore".to_string());
    } else {
        skipped.push(".gitignore".to_string());
    }

    let human = format!("created: {}\nskipped: {}\n", created.join(", "), skipped.join(", "));
    Ok(Report::ok(json!({"created": created, "skipped": skipped}), human))
}

/// Appends [`GITIGNORE_LINE`] to the `.gitignore` at `path` (created if missing) unless
/// it has it; returns whether it wrote. Bytes, not text, so a `.gitignore` that is not
/// UTF-8 is kept intact.
fn add_gitignore_line(path: &std::path::Path) -> Result<bool, Refusal> {
    let mut bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
        Err(e) => return Err(Refusal::io(format!("reading `.gitignore`: {e}"))),
    };
    if bytes.split(|b| *b == b'\n').any(|l| l.strip_suffix(b"\r").unwrap_or(l) == GITIGNORE_LINE.as_bytes()) {
        return Ok(false);
    }
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    bytes.extend_from_slice(format!("{GITIGNORE_LINE}\n").as_bytes());
    std::fs::write(path, bytes).map_err(Refusal::io)?;
    Ok(true)
}
