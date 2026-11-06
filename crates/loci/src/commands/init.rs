//! `loci init` (m0 §9.1).

use std::io::Write;
use std::path::Path;

use loci_core::config::{self, Config};
use loci_core::envelope::{Refusal, Report};
use loci_core::{git, path};
use serde_json::json;

use super::Ctx;
use crate::assets;

/// The line `.gitignore` needs: exports are written under `.loci/`.
const GITIGNORE_LINE: &str = "/.loci/";

pub fn run(ctx: &Ctx) -> Result<Report, Refusal> {
    // `init` creates the vault where it is told (the folder too, as `git init` does),
    // or right here: it never searches up.
    let dir = ctx.vault.dir().to_path_buf();
    std::fs::create_dir_all(&dir).map_err(Refusal::io_at(&dir))?;
    refuse_destination(&dir)?;
    ensure_repo_root(&dir)?;
    let (created, skipped) = write_files(&dir)?;
    let human = format!("created: {}\nskipped: {}\n", created.join(", "), skipped.join(", "));
    Ok(Report::ok(json!({"created": created, "skipped": skipped}), human))
}

/// Refuses a folder that is a vault already (`init.exists`), or where a file `init`
/// manages is a symlink (`file.symlink`): there is no vault to check yet (`Vault::open`
/// refuses symlinks), so this is checked before any file is written (m0 §3).
fn refuse_destination(dir: &Path) -> Result<(), Refusal> {
    if dir.join(config::FILE).symlink_metadata().is_ok() {
        return Err(Refusal::refused("init.exists", "loci.toml already exists"));
    }
    let managed = [path::RULES, path::STYLE, path::AGENTS, ".gitignore"];
    if let Some(name) = managed.into_iter().find(|n| dir.join(n).symlink_metadata().is_ok_and(|m| m.is_symlink())) {
        let message = format!("`{name}` is a symlink; loci does not follow symlinks");
        return Err(Refusal::refused("file.symlink", message));
    }
    Ok(())
}

/// Makes `dir` the root of a git work tree: `git init` when it is in none (or git
/// cannot tell, so `git init` fails loudly), a refusal (`vault.not-git`) when it is
/// inside one but not at its root.
fn ensure_repo_root(dir: &Path) -> Result<(), Refusal> {
    match git::toplevel(dir) {
        Err(_) => git::Repo::new(dir).init().map_err(Refusal::git),
        Ok(top) if dir.canonicalize().is_ok_and(|d| d == top) => Ok(()),
        Ok(_) => Err(Refusal::refused("vault.not-git", "run `loci init` at the root of the git work tree")),
    }
}

/// Writes the files a vault starts with, leaving any that exist; returns the names
/// created and skipped.
fn write_files(dir: &Path) -> Result<(Vec<String>, Vec<String>), Refusal> {
    let files: [(&str, String); 4] = [
        (config::FILE, Config::default_file(super::updates::all().last().expect("updates ship").id)),
        (path::RULES, "# Rules\n\n## Lessons\n".to_string()),
        (path::STYLE, "# Style\n".to_string()),
        (path::AGENTS, assets::AGENTS_STUB.to_string()),
    ];

    let (mut created, mut skipped) = (vec![], vec![]);
    for (name, content) in files {
        // `create_new`: an existing file (or symlink) is kept, never written through.
        let file = dir.join(name);
        match std::fs::File::create_new(&file) {
            Ok(mut f) => {
                f.write_all(content.as_bytes()).map_err(Refusal::io_at(&file))?;
                created.push(name.to_string());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => skipped.push(name.to_string()),
            Err(e) => return Err(Refusal::io_at(&file)(e)),
        }
    }

    if add_gitignore_line(&dir.join(".gitignore"))? {
        created.push(".gitignore".to_string());
    } else {
        skipped.push(".gitignore".to_string());
    }
    Ok((created, skipped))
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
    std::fs::write(path, bytes).map_err(Refusal::io_at(path))?;
    Ok(true)
}
