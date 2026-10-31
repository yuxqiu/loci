//! Core library of loci: vault model, parser and spans, link resolution, splice
//! editing, lint, digest, search, git helpers and the `--json` envelope.

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
pub mod sources;
pub mod splice;
pub mod vault;
