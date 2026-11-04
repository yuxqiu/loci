//! Helpers for tests: loci-core's unit tests, and other crates' tests through the
//! `testing` feature.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::Config;
use crate::index::Index;
use crate::parse::{Note, parse};
use crate::path::VaultPath;
use crate::vault::Vault;

/// A vault of in-memory files, (path, text): nothing on disk, a minimal config; `.md`
/// texts are its notes, other contents are unused.
pub fn vault(files: &[(impl AsRef<str>, impl AsRef<str>)]) -> Vault {
    let texts: BTreeMap<VaultPath, String> =
        files.iter().map(|(p, t)| (vp(p.as_ref()), t.as_ref().to_string())).collect();
    let files = texts.keys().cloned().collect();
    let texts = texts.into_iter().filter(|(p, _)| p.is_note()).collect();
    let config = Config::parse("schema = 1\n").expect("a minimal config parses");
    Vault::of(PathBuf::new(), config, files, texts)
}

/// The index of an in-memory vault of `files` ([`vault`]). The vault is leaked, so the
/// index outlives the test's statement.
pub fn index(files: &[(impl AsRef<str>, impl AsRef<str>)]) -> Index<'static> {
    Index::open(Box::leak(Box::new(vault(files))))
}

/// A concept note, at `concepts/a/x.md`, holding `text`.
pub fn concept(text: &str) -> Note {
    parse(&vp("concepts/a/x.md"), text.to_string())
}

/// `p` as a vault path; panics when it is none.
pub fn vp(p: &str) -> VaultPath {
    VaultPath::new(p.to_string()).unwrap_or_else(|| panic!("`{p}` is a vault path"))
}
