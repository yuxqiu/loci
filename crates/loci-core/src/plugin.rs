//! Plugin dispatch and the plugin contract (m0 §2.2, §2.3).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::envelope::{Diagnostic, Envelope, Refusal};

pub const ENV_VAULT: &str = "LOCI_VAULT";
pub const ENV_VERSION: &str = "LOCI_VERSION";

static NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9-]*$").unwrap());

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plugin {
    pub name: String,
    pub path: PathBuf,
}

/// A skill as reported by a plugin's `skills` hook (and as compiled into `loci`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub body: String,
}

impl Skill {
    /// A skill from its `SKILL.md`: the description is the `description:` field of the
    /// frontmatter (empty if absent).
    pub fn from_markdown(name: &str, body: &str) -> Self {
        // Parsed for its frontmatter only; the file is a plugin's, not the vault's.
        let path = crate::path::VaultPath::new("SKILL.md".to_string()).expect("one segment is a vault path");
        let note = crate::parse::parse(&path, crate::vault::Kind::Unknown, body.to_string());
        let description = note.fm_str("description").unwrap_or_default().to_string();
        Self { name: name.to_string(), description, body: body.to_string() }
    }
}

/// Every `loci-<name>` executable on `path_var`, first match per name, excluding core
/// command names. Sorted by name.
pub fn discover(path_var: Option<&std::ffi::OsStr>, core_commands: &[&str]) -> Vec<Plugin> {
    let mut found: BTreeMap<String, PathBuf> = BTreeMap::new();
    let Some(path_var) = path_var else { return vec![] };
    for dir in std::env::split_paths(path_var) {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut names: Vec<_> = entries.flatten().collect();
        names.sort_by_key(|e| e.file_name());
        for entry in names {
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str().and_then(|n| n.strip_prefix("loci-")) else { continue };
            if !NAME.is_match(name) || core_commands.contains(&name) || found.contains_key(name) {
                continue;
            }
            if is_executable(&entry.path()) {
                found.insert(name.to_string(), entry.path());
            }
        }
    }
    found.into_iter().map(|(name, path)| Plugin { name, path }).collect()
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Builds the command that runs a plugin with the dispatch environment.
pub fn command(plugin: &Plugin, vault_root: &Path, version: &str) -> Command {
    let mut c = Command::new(&plugin.path);
    c.env(ENV_VAULT, vault_root).env(ENV_VERSION, version).current_dir(vault_root);
    c
}

/// Runs a hook (`lint` or `skills`) with `--json` and validates the reply (§2.3 item 5).
/// On failure returns the reason for `plugin.failed`.
fn run_hook(plugin: &Plugin, hook: &str, vault_root: &Path, version: &str) -> Result<Envelope, String> {
    let out = command(plugin, vault_root, version)
        .args([hook, "--json"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {}: {e}", plugin.path.display()))?;
    // The plugin's own explanation, if it printed one, shortened.
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stderr = stderr.trim();
    let why = if stderr.is_empty() {
        String::new()
    } else {
        let cut = stderr.char_indices().nth(400).map_or(stderr.len(), |(i, _)| i);
        format!(": {}", &stderr[..cut])
    };
    match out.status.code() {
        Some(0 | 1) => {}
        Some(code) => return Err(format!("exited with code {code}{why}")),
        None => return Err(format!("was killed by a signal{why}")),
    }
    let env: Envelope =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("printed an invalid envelope: {e}{why}"))?;
    if let Some(e) = &env.error {
        return Err(format!("returned error {}: {}", e.code, e.message));
    }
    let prefix = format!("{}.", plugin.name);
    if let Some(d) = env.diagnostics.iter().find(|d| !d.code.starts_with(&prefix)) {
        return Err(format!("reported code `{}` without the `{prefix}` prefix", d.code));
    }
    Ok(env)
}

/// The plugin's lint diagnostics, or the reason the hook failed (for [`failed`]).
pub fn lint(plugin: &Plugin, vault_root: &Path, version: &str) -> Result<Vec<Diagnostic>, String> {
    run_hook(plugin, "lint", vault_root, version).map(|env| env.diagnostics)
}

/// The plugin's skills, or the reason the hook failed (for [`failed`]).
pub fn skills(plugin: &Plugin, vault_root: &Path, version: &str) -> Result<Vec<Skill>, String> {
    let env = run_hook(plugin, "skills", vault_root, version)?;
    #[derive(Deserialize)]
    struct Data {
        skills: Vec<Skill>,
    }
    let data: Data = serde_json::from_value(env.data).map_err(|e| format!("invalid data: {e}"))?;
    let prefix = format!("{}-", plugin.name);
    if let Some(s) = data.skills.iter().find(|s| !s.name.starts_with(&prefix)) {
        return Err(format!("skill `{}` lacks the `{prefix}` prefix", s.name));
    }
    Ok(data.skills)
}

/// The `plugin.failed` diagnostic for a hook failure.
pub fn failed(plugin: &Plugin, command: &str, reason: &str) -> Diagnostic {
    Diagnostic::error("plugin.failed", format!("plugin `{}` `{command}` failed: {reason}", plugin.name))
}

/// The environment a plugin runs in, checked on the plugin side (§2.3 item 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEnv {
    pub vault: PathBuf,
}

/// Checks `LOCI_VERSION` and `LOCI_VAULT`. `built_with` is the workspace version the
/// plugin was compiled with; the refusal goes into the plugin's envelope.
pub fn check_env(
    version: Option<String>,
    vault: Option<String>,
    name: &str,
    built_with: &str,
) -> Result<PluginEnv, Refusal> {
    let Some(version) = version else {
        return Err(Refusal::refused("plugin.direct", format!("run as `loci {name}`")));
    };
    let Some(vault) = vault else {
        return Err(Refusal::refused("vault.not-found", "LOCI_VAULT is not set"));
    };
    let bad = |m: String| Refusal::refused("plugin.version", m);
    let core = semver::Version::parse(&version).map_err(|e| bad(format!("bad LOCI_VERSION: {e}")))?;
    let req =
        semver::VersionReq::parse(&format!("^{built_with}")).map_err(|e| bad(format!("bad build version: {e}")))?;
    if !req.matches(&core) {
        return Err(bad(format!(
            "loci {version} is not compatible with the loci-core {built_with} this plugin was built with"
        )));
    }
    Ok(PluginEnv { vault: PathBuf::from(vault) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_rules() {
        let ok = |v: &str, b: &str| check_env(Some(v.into()), Some("/v".into()), "learn", b);
        assert!(ok("0.1.3", "0.1.0").is_ok());
        assert_eq!(ok("0.2.0", "0.1.0").unwrap_err().code(), "plugin.version");
        assert!(ok("1.4.0", "1.2.0").is_ok());
        assert_eq!(ok("2.0.0", "1.2.0").unwrap_err().code(), "plugin.version");
        assert_eq!(check_env(None, Some("/v".into()), "learn", "0.1.0").unwrap_err().code(), "plugin.direct");
        assert_eq!(check_env(Some("0.1.0".into()), None, "learn", "0.1.0").unwrap_err().code(), "vault.not-found");
    }

    #[test]
    fn discovery_skips_core_names_and_bad_names() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["loci-stub", "loci-lint", "loci-Bad", "other"] {
            let p = dir.path().join(n);
            std::fs::write(&p, "#!/bin/sh\n").unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let found = discover(Some(dir.path().as_os_str()), &["lint"]);
        assert_eq!(found.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["stub"]);
    }
}
