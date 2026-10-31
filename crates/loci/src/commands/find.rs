//! `loci find` (m0 §9.3).

use loci_core::envelope::{Outcome, Report};
use loci_core::index::Index;
use loci_core::parse::Note;
use loci_core::search::{self, SearchDoc};
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
        }
    }
}

fn doc(note: &Note) -> SearchDoc {
    SearchDoc {
        path: note.path.clone(),
        kind: note.kind,
        title: note.title().to_string(),
        aliases: note.fm_strings("aliases").into_iter().map(str::to_string).collect(),
        digest: note.fm_str("digest").unwrap_or_default().to_string(),
        body: note.text[note.body_start()..].to_string(),
    }
}

/// The search documents for every note core reads.
pub fn docs(index: &Index) -> Vec<SearchDoc> {
    index.notes.values().map(doc).collect()
}

/// A hit with its file line (hit lines from the search count within the body).
pub struct FileHit {
    pub hit: search::Hit,
    pub line: Option<usize>,
}

/// Runs one query; returns the hits with file line numbers.
pub fn query(index: &Index, docs: &[SearchDoc], q: &str, k: usize, kinds: &[Kind]) -> Result<Vec<FileHit>, String> {
    let hits = search::search(docs, q, k, kinds).map_err(|e| e.to_string())?;
    let hits = hits
        .into_iter()
        .map(|hit| {
            let offset = index.notes.get(&hit.path).map_or(0, |n| n.text[..n.body_start()].matches('\n').count());
            let line = hit.line.map(|l| l + offset);
            FileHit { hit, line }
        })
        .collect();
    Ok(hits)
}

pub fn run(ctx: &Ctx, q: &str, k: usize, kinds: &[Kind]) -> Result<Report, Report> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let hits =
        query(&index, &docs(&index), q, k, kinds).map_err(|e| Report::fail(Outcome::internal("find.index", e)))?;
    let mut human = String::new();
    let hits: Vec<_> = hits
        .iter()
        .map(|FileHit { hit: h, line }| {
            human.push_str(&match (line, &h.snippet) {
                (Some(l), Some(s)) => format!("{}:{l}: {s}\n", h.path),
                _ => format!("{}\n", h.path),
            });
            json!({"path": h.path, "kind": h.kind, "title": h.title, "score": h.score, "line": line, "snippet": h.snippet})
        })
        .collect();
    Ok(Report::new(Outcome::ok(json!({"query": q, "hits": hits})), human))
}
