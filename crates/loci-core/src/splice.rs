//! Splice editing (m0 §4.5): apply byte-range edits to an original text in one pass.

use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Byte range into the original text.
    pub range: Range<usize>,
    pub new: String,
}

impl Edit {
    pub fn replace(range: Range<usize>, new: impl Into<String>) -> Self {
        Self { range, new: new.into() }
    }

    pub fn insert(at: usize, new: impl Into<String>) -> Self {
        Self { range: at..at, new: new.into() }
    }
}

/// Why a set of edits cannot be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// An edit outside the text, reversed, or off a char boundary.
    Invalid(Range<usize>),
    /// Two edits overlap.
    Overlap(Range<usize>, Range<usize>),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::Invalid(r) => write!(f, "edit {r:?} is outside the text or off a character boundary"),
            EditError::Overlap(a, b) => write!(f, "edits {a:?} and {b:?} overlap"),
        }
    }
}

impl std::error::Error for EditError {}

/// Applies `edits` to `original`. Edits are sorted by start, an insert before a
/// replacement starting at the same offset; inserts at the same offset keep their input
/// order. Overlapping edits, or edits outside the text or off a char boundary, are an
/// error.
pub fn apply(original: &str, mut edits: Vec<Edit>) -> Result<String, EditError> {
    // Stable sort keeps the input order of inserts at the same offset.
    edits.sort_by_key(|e| (e.range.start, e.range.end));
    let mut out = String::with_capacity(original.len());
    let mut cursor = 0;
    let mut prev: Option<Range<usize>> = None;
    for edit in edits {
        let r = edit.range.clone();
        let bad = r.start > r.end
            || r.end > original.len()
            || !original.is_char_boundary(r.start)
            || !original.is_char_boundary(r.end);
        if bad {
            return Err(EditError::Invalid(r));
        }
        if let Some(p) = &prev {
            // Inserts at the end of a previous edit are fine; anything starting inside it is not.
            if r.start < p.end {
                return Err(EditError::Overlap(p.clone(), r));
            }
        }
        out.push_str(&original[cursor..r.start]);
        out.push_str(&edit.new);
        cursor = r.end;
        prev = Some(r);
    }
    out.push_str(&original[cursor..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_identity() {
        assert_eq!(apply("abc", vec![]).unwrap(), "abc");
    }

    #[test]
    fn replace_and_insert() {
        let out = apply("hello world", vec![Edit::replace(6..11, "there"), Edit::insert(0, ">> ")]).unwrap();
        assert_eq!(out, ">> hello there");
    }

    #[test]
    fn inserts_at_same_offset_keep_order() {
        let out = apply("ab", vec![Edit::insert(1, "1"), Edit::insert(1, "2")]).unwrap();
        assert_eq!(out, "a12b");
    }

    #[test]
    fn insert_goes_before_a_replacement_at_the_same_offset() {
        let out = apply("abcd", vec![Edit::replace(1..3, ""), Edit::insert(1, "X")]).unwrap();
        assert_eq!(out, "aXd");
    }

    #[test]
    fn overlap_is_error() {
        assert!(apply("abcdef", vec![Edit::replace(0..3, "x"), Edit::replace(2..4, "y")]).is_err());
    }

    #[test]
    fn adjacent_is_fine() {
        let out = apply("abcdef", vec![Edit::replace(0..2, "X"), Edit::replace(2..4, "Y")]).unwrap();
        assert_eq!(out, "XYef");
    }

    #[test]
    fn non_char_boundary_is_error() {
        assert!(apply("é", vec![Edit::insert(1, "x")]).is_err());
    }

    #[test]
    fn one_pass_equals_sequential_from_end() {
        let text = "the quick brown fox";
        let edits = vec![Edit::replace(4..9, "slow"), Edit::replace(16..19, "cat"), Edit::insert(0, "!")];
        let one = apply(text, edits.clone()).unwrap();
        let mut seq = text.to_string();
        let mut sorted = edits;
        sorted.sort_by_key(|e| std::cmp::Reverse(e.range.start));
        for e in sorted {
            seq = apply(&seq, vec![e]).unwrap();
        }
        assert_eq!(one, seq);
    }
}
