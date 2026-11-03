//! A fuzz property for the parser (research 18 §5): on markdown assembled from the
//! constructs loci reads, parsing never panics and every byte range it reports lies in
//! the text, on character boundaries, and inside the range that holds it. Every edit,
//! rename and fix relies on these offsets. A second property: rewriting a frontmatter
//! with no change keeps its fields, links and body.

use std::ops::Range;

use loci_core::parse::{Block, FmValue, Frontmatter, Node, Note, Spanned, parse};
use loci_core::path::VaultPath;
use proptest::prelude::*;

/// Pieces of the syntax loci parses, so generated text exercises it.
const PIECES: &[&str] = &[
    "---\n",
    "id: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n",
    "prereqs: [\"[[a]]\", \"[[二分]]\"]\n",
    "prereqs:\n  - \"[[b]]\"\n",
    "# ",
    "## ",
    "#",
    "===\n",
    "---",
    "\n",
    "\r\n",
    "\n\n",
    "- ",
    "1. ",
    "  - ",
    "> ",
    "```\n",
    "`",
    "$",
    "$$\n",
    "[[",
    "]]",
    "![[",
    "|",
    "\\|",
    "#",
    "^",
    "^abc1",
    " :: ",
    "::",
    "[x](a/b.md)",
    "![i](p.png)",
    "| a | b |\n",
    "|---|---|\n",
    "a",
    "bfs",
    "二分",
    "é",
    " ",
    "\t",
    "<div>",
    "</div>",
    "[^1]",
    "[^1]: n\n",
    "- [ ] ",
];

/// Frontmatter fields: comments, quoting styles, escapes, and values of each kind.
const FIELDS: &[&str] = &[
    "id: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n",
    "# a comment\n",
    "digest: \"é \\\"x\\\"\"\n",
    "note: plain text\n",
    "n: 12\n",
    "flag: yes\n",
    "empty:\n",
    "nested: {a: 1}\n",
    "multi: >\n  folded\n  lines\n",
];

/// The forms of `prereqs`: quoting styles, an escape, a comment, unquoted.
const PREREQS: &[&str] = &[
    "",
    "prereqs: [\"[[a]]\", '[[二分]]']\n",
    "prereqs:\n  - \"[[b]]\" # why\n  - \"[[\\u0063]]\"\n",
    "prereqs: [[a]]\n",
    "prereqs: []\n",
];

/// A note that starts with a frontmatter: some `FIELDS`, shuffled, and a `PREREQS`.
fn with_frontmatter() -> impl Strategy<Value = String> {
    let fields = proptest::sample::subsequence(FIELDS, 0..FIELDS.len()).prop_shuffle();
    (fields, proptest::sample::select(PREREQS), markdown())
        .prop_map(|(fields, prereqs, body)| format!("---\n{}{prereqs}---\n{body}", fields.concat()))
}

fn markdown() -> impl Strategy<Value = String> {
    let piece = prop_oneof![4 => proptest::sample::select(PIECES).prop_map(str::to_string), 1 => "\\PC{0,4}"];
    proptest::collection::vec(piece, 0..40).prop_map(|ps| ps.concat())
}

/// What a rewrite must keep of a note: each field's key and value (string values, list
/// shapes), the links' targets, and the body but for its leading blank lines (which go
/// with a removed block, m0 §6.1).
fn meaning(n: &Note) -> (Vec<(String, String)>, Vec<String>, &str) {
    fn value(v: &FmValue) -> String {
        match v {
            FmValue::Str(value) => format!("{value:?}"),
            FmValue::List(items) => format!("[{}]", items.iter().map(|i| value(i)).collect::<Vec<_>>().join(",")),
            FmValue::Other => "other".into(),
        }
    }
    let fields =
        n.frontmatter.iter().flat_map(|f| f.entries()).map(|e| (e.key.node.clone(), value(&e.value))).collect();
    let links = n.links.iter().map(|l| format!("{:?} {:?}", l.form, l.target.names)).collect();
    let body = &n.text[n.body_start()..];
    let blank: usize =
        body.split_inclusive('\n').take_while(|l| l.trim().is_empty() && l.ends_with('\n')).map(str::len).sum();
    (fields, links, &body[blank..])
}

fn check(text: &str, r: &Range<usize>, within: &Range<usize>, what: &str) -> Result<(), TestCaseError> {
    prop_assert!(r.start <= r.end && within.start <= r.start && r.end <= within.end, "{what} {r:?} outside {within:?}");
    prop_assert!(text.is_char_boundary(r.start) && text.is_char_boundary(r.end), "{what} {r:?} splits a character");
    Ok(())
}

fn check_value(text: &str, v: &Spanned<FmValue>, within: &Range<usize>) -> Result<(), TestCaseError> {
    check(text, &v.span, within, "frontmatter value")?;
    if let FmValue::List(items) = &v.node {
        for item in items {
            check_value(text, item, &v.span)?;
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn every_span_is_in_the_text(text in markdown()) {
        let path = VaultPath::new("concepts/a/x.md".into()).unwrap();
        let n = parse(&path, text.clone());
        let all = 0..text.len();
        for l in &n.links {
            check(&text, &l.span, &all, "link")?;
            check(&text, &l.target.span, &l.span, "link target")?;
            if let Some(label) = &l.label {
                check(&text, &label.span, &l.span, "link label")?;
            }
        }
        for node in n.descendants() {
            match node {
                Node::Block(b) => {
                    check(&text, &b.span, &all, "block")?;
                    match &b.node {
                        Block::Heading(h) => check(&text, &h.content.span, &b.span, "heading content")?,
                        Block::Paragraph { id: Some(id) } => check(&text, &id.span, &b.span, "paragraph id")?,
                        Block::List(items) => {
                            for i in items {
                                check(&text, &i.span, &b.span, "item")?;
                            }
                        }
                        Block::Other(children) => {
                            for c in children {
                                check(&text, &c.span, &b.span, "nested block")?;
                            }
                        }
                        Block::Paragraph { id: None } => {}
                    }
                }
                Node::Item(i) => {
                    check(&text, &i.lead.span, &i.span, "item lead")?;
                    check(&text, &i.marker.span, &i.span, "item marker")?;
                    if let Some(id) = &i.id {
                        check(&text, &id.span, &i.span, "item id")?;
                    }
                    for c in &i.children {
                        check(&text, &c.span, &i.span, "item block")?;
                    }
                }
            }
        }
        for m in &n.markdown_links {
            check(&text, &m.span, &all, "markdown link")?;
            check(&text, &m.label.span, &m.span, "markdown link label")?;
        }
        if let Some(fm) = &n.frontmatter {
            check(&text, &fm.span, &all, "frontmatter")?;
            for e in fm.entries() {
                check(&text, &e.key.span, &fm.span, "frontmatter key")?;
                check_value(&text, &e.value, &fm.span)?;
            }
        }
    }

    #[test]
    fn a_rewrite_keeps_the_meaning(text in with_frontmatter()) {
        let path = VaultPath::new("concepts/a/x.md".into()).unwrap();
        let n = parse(&path, text.clone());
        prop_assume!(n.frontmatter.as_ref().is_some_and(|f| matches!(f.node, Frontmatter::Fields { .. })));
        let edit = n.rewrite_frontmatter(&[]);
        let out = loci_core::splice::apply(&text, vec![edit]).unwrap();
        let after = parse(&path, out.clone());
        prop_assert_eq!(meaning(&n), meaning(&after), "rewritten:\n{}", out);
    }
}
