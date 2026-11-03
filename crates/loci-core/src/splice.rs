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

/// Applies `edits` to `original`. Edits are sorted by start, an insert before a
/// replacement starting at the same offset; inserts at the same offset keep their input
/// order (the sort is stable). Overlapping edits, or edits outside the text or off a char
/// boundary, are an error.
pub fn apply(original: &str, mut edits: Vec<Edit>) -> Result<String, String> {
    edits.sort_by_key(|e| (e.range.start, e.range.end));
    let mut out = String::with_capacity(original.len());

    // The end of the previous edit: an edit may start there (an insert), not before.
    let mut cursor = 0;
    for edit in edits {
        let r = edit.range;
        if r.start > r.end
            || r.end > original.len()
            || !original.is_char_boundary(r.start)
            || !original.is_char_boundary(r.end)
        {
            return Err(format!("edit {r:?} is outside the text or off a character boundary"));
        }
        if r.start < cursor {
            return Err(format!("edit {r:?} overlaps the one before it, which ends at {cursor}"));
        }
        out.push_str(&original[cursor..r.start]);
        out.push_str(&edit.new);
        cursor = r.end;
    }

    out.push_str(&original[cursor..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn non_char_boundary_is_error() {
        assert!(apply("é", vec![Edit::insert(1, "x")]).is_err());
    }
}
