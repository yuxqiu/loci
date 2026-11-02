//! A fuzz property for the parser (research 18 §5): on markdown assembled from the
//! constructs loci reads, parsing never panics and every byte range it reports lies in
//! the text, on character boundaries, and inside the range that holds it. Every edit,
//! rename and fix relies on these offsets.

use std::ops::Range;

use loci_core::parse::{FmValue, parse};
use loci_core::path::VaultPath;
use loci_core::vault::Kind;
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

fn markdown() -> impl Strategy<Value = String> {
    let piece = prop_oneof![4 => proptest::sample::select(PIECES).prop_map(str::to_string), 1 => "\\PC{0,4}"];
    proptest::collection::vec(piece, 0..40).prop_map(|ps| ps.concat())
}

fn check(text: &str, r: &Range<usize>, within: &Range<usize>, what: &str) -> Result<(), TestCaseError> {
    prop_assert!(r.start <= r.end && within.start <= r.start && r.end <= within.end, "{what} {r:?} outside {within:?}");
    prop_assert!(text.is_char_boundary(r.start) && text.is_char_boundary(r.end), "{what} {r:?} splits a character");
    Ok(())
}

fn check_value(text: &str, v: &FmValue, within: &Range<usize>) -> Result<(), TestCaseError> {
    check(text, &v.span(), within, "frontmatter value")?;
    if let FmValue::List { items, span, .. } = v {
        for item in items {
            check_value(text, item, span)?;
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn every_span_is_in_the_text(text in markdown()) {
        let path = VaultPath::new("concepts/a/x.md".into()).unwrap();
        let n = parse(&path, Kind::Concept, text.clone());
        let all = 0..text.len();
        for l in &n.links {
            check(&text, &l.range, &all, "link")?;
            check(&text, &l.target_range, &l.range, "link target")?;
            prop_assert_eq!(&text[l.target_range.clone()], l.raw.as_str());
            if let Some(label) = &l.label {
                check(&text, label, &l.range, "link label")?;
            }
        }
        for h in &n.headings {
            check(&text, &h.range, &all, "heading")?;
            check(&text, &h.content, &h.range, "heading content")?;
        }
        for i in &n.items {
            check(&text, &i.range, &all, "item")?;
            check(&text, &i.lead, &i.range, "item lead")?;
            prop_assert!(i.range.start <= i.content_start && i.content_start <= i.range.end, "item content start");
        }
        for b in &n.block_ids {
            check(&text, &b.block, &all, "block")?;
            check(&text, &b.range, &b.block, "block id")?;
        }
        for (_, r) in &n.top_blocks {
            check(&text, r, &all, "top-level block")?;
        }
        for m in &n.md_links {
            check(&text, &m.range, &all, "markdown link")?;
        }
        if let Some(fm) = &n.frontmatter {
            check(&text, &fm.range, &all, "frontmatter")?;
            for e in &fm.entries {
                check(&text, &e.key_span, &fm.range, "frontmatter key")?;
                check_value(&text, &e.value, &fm.range)?;
            }
        }
    }
}
