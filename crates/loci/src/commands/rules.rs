//! `loci rules` (m0 §9.10): the framework rules, read at session start.

use loci_core::envelope::{Refusal, Report};
use serde_json::json;

use crate::assets;

/// Needs no vault: the rules are compiled into the binary.
pub fn run() -> Result<Report, Refusal> {
    Ok(Report::ok(json!({"rules": assets::RULES}), assets::RULES))
}
