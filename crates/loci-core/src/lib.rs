//! Core library of loci: the vault model, parsing with byte spans, link resolution,
//! splice editing, lint, digest, search, link-preserving changes (`mv`, `merge`),
//! prerequisite closure, export, plugins, git helpers and the `--json` envelope.

pub mod blockids;
pub mod closure;
pub mod config;
pub mod digest;
pub mod envelope;
pub mod export;
pub mod git;
pub mod index;
pub mod lint;
pub mod merge;
pub mod mv;
pub mod parse;
pub mod plugin;
pub mod relink;
pub mod search;
pub mod splice;
pub mod vault;
