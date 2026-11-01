//! `loci find` (m0 §9.3).

use loci_core::envelope::{Refusal, Report};
use loci_core::index::Index;
use loci_core::search;
use loci_core::vault::Kind;
use serde_json::json;

use super::Ctx;

/// The kinds `--kind` accepts (m0 §9.3).
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum KindArg {
    Concept,
    Area,
    View,
    Journal,
    Source,
    Inbox,
    Rules,
    /// Markdown outside every known folder (`kind.unknown`).
    Unknown,
}

impl From<KindArg> for Kind {
    fn from(k: KindArg) -> Kind {
        match k {
            KindArg::Concept => Kind::Concept,
            KindArg::Area => Kind::Area,
            KindArg::View => Kind::View,
            KindArg::Journal => Kind::Journal,
            KindArg::Source => Kind::Source,
            KindArg::Inbox => Kind::Inbox,
            KindArg::Rules => Kind::Rules,
            KindArg::Unknown => Kind::Unknown,
        }
    }
}

pub fn run(ctx: &Ctx, q: &str, k: usize, kinds: &[Kind]) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let hits = search::search(&search::docs(&index), q, k, kinds)?;
    let human: String = hits
        .iter()
        .map(|h| match (h.line, &h.snippet) {
            (Some(l), Some(s)) => format!("{}:{l}: {s}\n", h.path),
            _ => format!("{}\n", h.path),
        })
        .collect();
    Ok(Report::ok(json!({"query": q, "hits": hits}), human))
}
