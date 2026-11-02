//! Core commands. Each `run` returns its report, or the refusal that stopped it;
//! `dispatch` runs a plugin instead.

pub mod bench;
pub mod blockids;
pub mod closure;
pub mod dispatch;
pub mod done;
pub mod export;
pub mod init;
pub mod links;
pub mod lint;
pub mod merge;
pub mod mv;
pub mod new;
pub mod rules;
pub mod skill;
pub mod status;
pub mod updates;

use std::path::PathBuf;

use loci_core::envelope::Refusal;
use loci_core::plugin::{self, Plugin};
use loci_core::vault::Vault;

pub struct Ctx {
    pub explicit_vault: Option<PathBuf>,
    pub cwd: PathBuf,
    pub json: bool,
}

impl Ctx {
    /// Opens the vault, or refuses (m0 §8, global options).
    pub fn vault(&self) -> Result<Vault, Refusal> {
        Vault::open(self.explicit_vault.as_deref(), &self.cwd)
    }
}

/// The files a planned change would touch, one per line: renames, edits (with how many),
/// deletions.
pub fn plan_lines(change: &loci_core::relink::Change) -> String {
    let moves = change.moves.iter().map(|(from, to)| format!("  rename {from} -> {to}\n"));
    let writes = change.writes.iter().map(|(p, w)| format!("  edit {p} ({} edit(s))\n", w.edits));
    let deleted = change.deleted.iter().map(|p| format!("  delete {p}\n"));
    moves.chain(writes).chain(deleted).collect()
}

/// A change's edited files, `[{"path", "edits"}]` (the `edited` of `mv` and `merge`).
pub fn edited(change: &loci_core::relink::Change) -> serde_json::Value {
    change.writes.iter().map(|(p, w)| serde_json::json!({"path": p, "edits": w.edits})).collect()
}

/// Installed plugins (m0 §2.2).
pub fn plugins() -> Vec<Plugin> {
    let core = crate::core_commands();
    plugin::discover(std::env::var_os("PATH").as_deref(), &core.iter().map(String::as_str).collect::<Vec<_>>())
}
