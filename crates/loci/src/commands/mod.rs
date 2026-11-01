//! Core commands. Each `run` returns its report, or the refusal that stopped it;
//! `dispatch` runs a plugin instead.

pub mod bench;
pub mod blockids;
pub mod closure;
pub mod digest;
pub mod dispatch;
pub mod done;
pub mod export;
pub mod fetch;
pub mod find;
pub mod init;
pub mod links;
pub mod lint;
pub mod merge;
pub mod mv;
pub mod new;
pub mod skill;
pub mod status;

use std::path::PathBuf;

use loci_core::envelope::Refusal;
use loci_core::plugin::{self, Plugin};
use loci_core::vault::{self, Vault};

use crate::CORE_COMMANDS;

pub struct Ctx {
    pub explicit_vault: Option<PathBuf>,
    pub cwd: PathBuf,
    pub json: bool,
}

impl Ctx {
    /// Opens the vault, or refuses (m0 §8, global options).
    pub fn vault(&self) -> Result<Vault, Refusal> {
        Ok(Vault::open(self.explicit_vault.as_deref(), &self.cwd)?)
    }
}

/// Installed plugins (m0 §2.2).
pub fn plugins() -> Vec<Plugin> {
    plugin::discover(std::env::var_os("PATH").as_deref(), CORE_COMMANDS)
}

/// A user-given vault path (m0 §8), or the `path.outside` refusal.
pub fn vault_path(p: &str) -> Result<String, Refusal> {
    vault::user_path(p).map_err(|m| Refusal::refused("path.outside", m))
}
