//! Full-text search (`loci find`, m0 §9.3): an in-RAM tantivy index built on each
//! call, tokenized with the `loci` tokenizer, ranked by BM25 with ties broken by
//! path bytes.

mod tokenizer;

use std::collections::HashSet;
use std::fmt;

use serde::Serialize;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, Query, TermQuery};
use tantivy::schema::{Field, IndexRecordOption, STORED, Schema, TextFieldIndexing, TextOptions, Value};
use tantivy::{Index, TantivyDocument, Term};
use unicode_normalization::UnicodeNormalization;

use crate::vault::Kind;

pub use tokenizer::{LociTokenStream, LociTokenizer, tokenize};

const TOKENIZER: &str = "loci";
const SNIPPET_MAX_CHARS: usize = 200;

/// One indexed file. The caller fills the fields per m0 §9.3.
#[derive(Debug, Clone)]
pub struct SearchDoc {
    /// Vault-relative path; also the tie-breaker for equal scores.
    pub path: String,
    /// Matched by the kind filter.
    pub kind: Kind,
    /// H1 raw text, else the file stem.
    pub title: String,
    pub aliases: Vec<String>,
    pub digest: String,
    /// Body text with frontmatter excluded.
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hit {
    pub path: String,
    pub kind: Kind,
    pub title: String,
    pub score: f32,
    /// 1-based line in [`SearchDoc::body`] of the first line containing a query
    /// token. The caller adds the frontmatter's line count for a file line number.
    pub line: Option<usize>,
    /// That line (NFC), trimmed to at most 200 characters around the first match.
    pub snippet: Option<String>,
}

#[derive(Debug)]
pub struct SearchError(tantivy::TantivyError);

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "search index: {}", self.0)
    }
}

impl std::error::Error for SearchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<tantivy::TantivyError> for SearchError {
    fn from(e: tantivy::TantivyError) -> Self {
        SearchError(e)
    }
}

/// Returns the top `k` of `docs` for `query`. An empty `kinds` keeps every kind.
pub fn search(docs: &[SearchDoc], query: &str, k: usize, kinds: &[Kind]) -> Result<Vec<Hit>, SearchError> {
    let tokenizer = LociTokenizer::new();
    let docs: Vec<&SearchDoc> = docs.iter().filter(|d| kinds.is_empty() || kinds.contains(&d.kind)).collect();

    let mut sb = Schema::builder();
    let text = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(TOKENIZER)
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let boosted: [(Field, f32); 5] = [
        (sb.add_text_field("title", text.clone()), 3.0),
        (sb.add_text_field("aliases", text.clone()), 3.0),
        (sb.add_text_field("digest", text.clone()), 2.0),
        // The tokenizer splits on every non-alphanumeric, including `/ - _ .`.
        (sb.add_text_field("path", text.clone()), 1.0),
        (sb.add_text_field("body", text), 1.0),
    ];
    let [title, aliases, digest, path, body] = boosted.map(|(f, _)| f);
    let id = sb.add_u64_field("id", STORED);
    let index = Index::create_in_ram(sb.build());
    index.tokenizers().register(TOKENIZER, tokenizer.clone());

    // One thread keeps doc ids deterministic.
    let mut writer = index.writer_with_num_threads(1, 15_000_000)?;
    for (i, d) in docs.iter().enumerate() {
        let mut doc = TantivyDocument::default();
        doc.add_text(title, &d.title);
        for a in &d.aliases {
            doc.add_text(aliases, a);
        }
        doc.add_text(digest, &d.digest);
        doc.add_text(path, &d.path);
        doc.add_text(body, &d.body);
        doc.add_u64(id, i as u64);
        writer.add_document(doc)?;
    }
    writer.commit()?;

    let mut terms: Vec<String> = Vec::new();
    for t in tokenize_with(&tokenizer, query).into_iter().map(|t| t.text) {
        if !terms.contains(&t) {
            terms.push(t);
        }
    }
    let mut hits = Vec::new();
    if !terms.is_empty() && !docs.is_empty() && k > 0 {
        let clauses: Vec<(Occur, Box<dyn Query>)> = boosted
            .iter()
            .flat_map(|&(field, boost)| {
                terms.iter().map(move |t| {
                    let q = TermQuery::new(Term::from_field_text(field, t), IndexRecordOption::WithFreqs);
                    (Occur::Should, Box::new(BoostQuery::new(Box::new(q), boost)) as Box<dyn Query>)
                })
            })
            .collect();
        let searcher = index.reader()?.searcher();
        // Collect every match so ties at the k-th place are broken by path.
        let top = searcher.search(&BooleanQuery::new(clauses), &TopDocs::with_limit(docs.len()).order_by_score())?;
        let mut scored = Vec::with_capacity(top.len());
        for (score, addr) in top {
            let stored: TantivyDocument = searcher.doc(addr)?;
            let i = stored.get_first(id).and_then(|v| v.as_u64()).expect("stored id") as usize;
            scored.push((score, docs[i]));
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.path.as_bytes().cmp(b.1.path.as_bytes())));
        scored.truncate(k);
        let term_set: HashSet<&str> = terms.iter().map(String::as_str).collect();
        hits = scored
            .into_iter()
            .map(|(score, d)| {
                let (line, snippet) = match first_match(&tokenizer, &d.body, &term_set) {
                    Some((line, snippet)) => (Some(line), Some(snippet)),
                    None => (None, None),
                };
                Hit { path: d.path.clone(), kind: d.kind, title: d.title.clone(), score, line, snippet }
            })
            .collect();
    }
    Ok(hits)
}

fn tokenize_with(tokenizer: &LociTokenizer, text: &str) -> Vec<tantivy::tokenizer::Token> {
    use tantivy::tokenizer::{TokenStream, Tokenizer};
    let mut t = tokenizer.clone();
    let mut stream = t.token_stream(text);
    let mut out = Vec::new();
    while stream.advance() {
        out.push(stream.token().clone());
    }
    out
}

/// The first body line containing a query token: its 1-based number and snippet.
fn first_match(tokenizer: &LociTokenizer, body: &str, terms: &HashSet<&str>) -> Option<(usize, String)> {
    body.lines().enumerate().find_map(|(n, line)| {
        let line: String = line.nfc().collect();
        let line = line.trim();
        let tok = tokenize_with(tokenizer, line).into_iter().find(|t| terms.contains(t.text.as_str()))?;
        Some((n + 1, snippet(line, tok.offset_from)))
    })
}

/// At most [`SNIPPET_MAX_CHARS`] characters of `line` around byte offset `at`.
fn snippet(line: &str, at: usize) -> String {
    let len = line.chars().count();
    if len <= SNIPPET_MAX_CHARS {
        return line.to_string();
    }
    let at_char = line[..at].chars().count();
    let start = at_char.saturating_sub(SNIPPET_MAX_CHARS / 2).min(len - SNIPPET_MAX_CHARS);
    line.chars().skip(start).take(SNIPPET_MAX_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(path: &str, kind: Kind, title: &str, body: &str) -> SearchDoc {
        SearchDoc {
            path: path.into(),
            kind,
            title: title.into(),
            aliases: vec![],
            digest: String::new(),
            body: body.into(),
        }
    }

    fn paths(hits: &[Hit]) -> Vec<&str> {
        hits.iter().map(|h| h.path.as_str()).collect()
    }

    #[test]
    fn title_outranks_body() {
        let docs = [
            doc("concepts/a.md", Kind::Concept, "Other", "dijkstra appears in the body"),
            doc("concepts/b.md", Kind::Concept, "Dijkstra", "shortest paths"),
            doc("concepts/c.md", Kind::Concept, "Unrelated", "nothing here"),
        ];
        let out = search(&docs, "Dijkstra", 10, &[]).unwrap();
        assert_eq!(paths(&out), ["concepts/b.md", "concepts/a.md"]);
    }

    #[test]
    fn alias_and_path_match() {
        let mut aliased = doc("concepts/x.md", Kind::Concept, "X", "");
        aliased.aliases = vec!["Breadth-first search".into()];
        let docs = [aliased, doc("concepts/breadth-first.md", Kind::Concept, "Y", "")];
        let out = search(&docs, "breadth", 10, &[]).unwrap();
        // Alias ×3 outranks path ×1.
        assert_eq!(paths(&out), ["concepts/x.md", "concepts/breadth-first.md"]);
    }

    #[test]
    fn ties_broken_by_path_and_k_truncates() {
        let docs = [
            doc("c.md", Kind::Concept, "T", "graph"),
            doc("a.md", Kind::Concept, "T", "graph"),
            doc("b.md", Kind::Concept, "T", "graph"),
        ];
        let out = search(&docs, "graph", 2, &[]).unwrap();
        assert_eq!(paths(&out), ["a.md", "b.md"]);
        assert_eq!(out[0].score, out[1].score);
    }

    #[test]
    fn kind_filter() {
        let docs = [
            doc("concepts/g.md", Kind::Concept, "Graph", ""),
            doc("journal/g.md", Kind::Journal, "Graph", ""),
            doc("inbox/g.md", Kind::Inbox, "Graph", ""),
        ];
        let kinds = [Kind::Journal, Kind::Inbox];
        let out = search(&docs, "graph", 10, &kinds).unwrap();
        assert_eq!(paths(&out), ["inbox/g.md", "journal/g.md"]);
    }

    #[test]
    fn no_match_and_empty_query() {
        let docs = [doc("a.md", Kind::Concept, "A", "text")];
        assert!(search(&docs, "zzz", 10, &[]).unwrap().is_empty());
        assert!(search(&docs, "  --  ", 10, &[]).unwrap().is_empty());
        assert!(search(&docs, "text", 0, &[]).unwrap().is_empty());
    }

    #[test]
    fn line_and_snippet() {
        let docs = [doc("a.md", Kind::Concept, "A", "first line\n\n   Uses a HEAP for speed.  \nheap again")];
        let hit = &search(&docs, "heap", 10, &[]).unwrap()[0];
        assert_eq!(hit.line, Some(3));
        assert_eq!(hit.snippet.as_deref(), Some("Uses a HEAP for speed."));

        let title_only = [doc("b.md", Kind::Concept, "Heap", "no match in body")];
        let hit = &search(&title_only, "heap", 10, &[]).unwrap()[0];
        assert_eq!((hit.line, hit.snippet.as_deref()), (None, None));
    }

    #[test]
    fn long_line_snippet_is_trimmed_around_match() {
        let line = format!("{} needle {}", "a ".repeat(300), "b ".repeat(300));
        let docs = [doc("a.md", Kind::Concept, "A", &line)];
        let snip = search(&docs, "needle", 10, &[]).unwrap()[0].snippet.clone().unwrap();
        assert_eq!(snip.chars().count(), SNIPPET_MAX_CHARS);
        assert!(snip.contains("needle"));
        // Match near the end: the window ends at the line's end.
        assert_eq!(snippet(&"x".repeat(300), 299), "x".repeat(200));
    }

    #[test]
    fn cjk_search() {
        let docs = [
            doc("zh.md", Kind::Concept, "梯度下降", "机器学习中的梯度下降算法"),
            doc("en.md", Kind::Concept, "Gradient", "gradient descent"),
        ];
        let out = search(&docs, "梯度", 10, &[]).unwrap();
        assert_eq!(paths(&out), ["zh.md"]);
        assert_eq!(out[0].line, Some(1));
    }

    #[test]
    fn kanji_only_query_finds_japanese_text() {
        let docs = [doc("ja.md", Kind::View, "日本語", "東京大学でアルゴリズムを勉強しています。")];
        assert_eq!(paths(&search(&docs, "勉強", 10, &[]).unwrap()), ["ja.md"]);
    }

    #[test]
    fn korean_word_inside_an_inflected_word() {
        let docs = [doc("ko.md", Kind::View, "한국어", "저는한국어를공부합니다")];
        assert_eq!(paths(&search(&docs, "공부", 10, &[]).unwrap()), ["ko.md"]);
    }
}
