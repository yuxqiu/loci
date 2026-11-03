//! Helpers shared by the unit tests.

use std::collections::BTreeMap;

use crate::index::Index;
use crate::parse::{Note, parse};
use crate::path::VaultPath;

/// A concept note, at `concepts/a/x.md`, holding `text`.
pub(crate) fn concept(text: &str) -> Note {
    parse(&VaultPath::new("concepts/a/x.md".into()).unwrap(), text.to_string())
}

/// An index of in-memory files, (path, text): `.md` texts are parsed, other contents
/// are unused.
pub(crate) fn index(files: &[(impl AsRef<str>, impl AsRef<str>)]) -> Index {
    let map: BTreeMap<VaultPath, &str> =
        files.iter().map(|(p, t)| (VaultPath::new(p.as_ref().to_string()).unwrap(), t.as_ref())).collect();
    Index::build(map.keys().cloned().collect(), |p| map[p].to_string())
}
