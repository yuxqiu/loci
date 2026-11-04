//! Core library of loci: the vault model, parsing with byte spans, link resolution,
//! splice editing, lint and its fixes, link-preserving changes (`mv`, `merge`), block ids,
//! prerequisite closure, export, plugins, git helpers and the `--json` envelope.

pub mod blockids;
pub mod closure;
pub mod config;
pub mod envelope;
pub mod export;
pub mod fix;
pub mod git;
pub mod id;
pub mod index;
pub mod lint;
pub mod merge;
pub mod mv;
pub mod parse;
pub mod path;
pub mod plugin;
pub mod relink;
pub mod splice;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod vault;
