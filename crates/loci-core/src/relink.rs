//! Link-preserving changes shared by `mv` and `merge`: rewrite links whose targets move,
//! check the result in memory, and apply it.

use std::collections::{BTreeMap, BTreeSet};

use crate::envelope::Refusal;
use crate::index::{self, Index};
use crate::parse::{Link, Note};
use crate::path::VaultPath;
use crate::splice::{self, Edit};
use crate::vault::{self, Kind, Vault};

/// A computed change to the vault.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Change {
    /// Files renamed on disk: old path -> new path.
    pub moves: BTreeMap<VaultPath, VaultPath>,
    /// Files deleted from disk.
    pub deleted: BTreeSet<VaultPath>,
    /// Every edited file, keyed by its final path.
    pub writes: BTreeMap<VaultPath, Written>,
    /// Links removed on purpose, as (original path, index into that note's `links`).
    pub removed_links: BTreeSet<(VaultPath, usize)>,
}

/// The new content of an edited file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub text: String,
    /// How many edits made it.
    pub edits: usize,
}

impl Change {
    fn final_path(&self, path: &VaultPath) -> VaultPath {
        self.moves.get(path).unwrap_or(path).clone()
    }
}

/// Builds the edits for every link, in files outside `sources/` that `change` does not
/// delete, whose resolved target is a key of `targets` (old target path -> new target
/// path).
pub fn link_edits(
    index: &Index,
    targets: &BTreeMap<VaultPath, VaultPath>,
    change: &Change,
) -> BTreeMap<VaultPath, Vec<Edit>> {
    let mut edits: BTreeMap<VaultPath, Vec<Edit>> = BTreeMap::new();
    for note in index.notes().filter(|n| n.kind != Kind::Source && !change.deleted.contains(&n.path)) {
        for link in &note.links {
            if let Some(new_raw) = rewrite(index, note, link, targets) {
                edits.entry(note.path.clone()).or_default().push(Edit::replace(link.target_range.clone(), new_raw));
            }
        }
    }
    edits
}

/// The new raw target for a link whose target moves, keeping its form.
fn rewrite(index: &Index, note: &Note, link: &Link, targets: &BTreeMap<VaultPath, VaultPath>) -> Option<String> {
    if link.target.is_empty() {
        return None; // `[[#frag]]` follows its own note.
    }
    // The target without its fragment: a link whose fragment no longer resolves still
    // follows its note (lint reports the fragment).
    let new = targets.get(&index.link_target(note, link)?)?;
    let mut raw = if !link.target.contains('/') {
        new.stem().to_string()
    } else {
        // A note is linked without `.md`, any other file with its extension.
        new.as_str().strip_suffix(".md").unwrap_or(new.as_str()).to_string()
    };
    // The fragment stays (a prerequisite, a bare `"[[slug]]"`, has none); a table's
    // escaping `\` is outside the target, so it stays too.
    if let Some(f) = &link.fragment {
        raw.push_str(&format!("#{f}"));
    }
    (raw != link.raw).then_some(raw)
}

/// Completes `change` (its moves, deletions and removed links set) with per-file
/// `edits`, then verifies it ([`verify`], refusing with `code`).
pub fn finish(
    index: &Index,
    mut change: Change,
    targets: &BTreeMap<VaultPath, VaultPath>,
    edits: BTreeMap<VaultPath, Vec<Edit>>,
    code: &'static str,
) -> Result<Change, Refusal> {
    for (path, file_edits) in edits {
        let note = index.parsed(&path).expect("edits are only made to readable notes");
        let count = file_edits.len();
        let text =
            splice::apply(&note.text, file_edits).map_err(|e| Refusal::internal("change.edit", e.to_string()))?;
        change.writes.insert(change.final_path(&path), Written { text, edits: count });
    }
    verify(index, targets, &change, code)?;
    Ok(change)
}

/// Builds the changed tree in memory and checks that every link outside `sources/` that
/// resolved before resolves to `targets`' image of its old target (m0 §9.5 step 3).
/// A fragment into a deleted note (the absorbed note of a merge) may stop resolving.
fn verify(
    index: &Index,
    targets: &BTreeMap<VaultPath, VaultPath>,
    change: &Change,
    code: &'static str,
) -> Result<(), Refusal> {
    let mut files: Vec<VaultPath> =
        index.files.iter().filter(|f| !change.deleted.contains(*f)).map(|f| change.final_path(f)).collect();
    files.sort();
    let contents: BTreeMap<VaultPath, String> = index
        .notes()
        .filter(|n| !change.deleted.contains(&n.path))
        .map(|n| {
            let p = change.final_path(&n.path);
            let text = change.writes.get(&p).map_or_else(|| n.text.clone(), |w| w.text.clone());
            (p, text)
        })
        .collect();
    let after = index::build(files, |p| contents[p].clone());
    for note in index.notes().filter(|n| n.kind != Kind::Source && !change.deleted.contains(&n.path)) {
        let new_path = change.final_path(&note.path);
        let Some(new_note) = after.parsed(&new_path) else {
            return Err(Refusal::refused(code, format!("{new_path} would no longer parse as a note")));
        };
        if note.fm_error.is_none() && new_note.fm_error.is_some() {
            return Err(Refusal::refused(code, format!("{new_path} would get invalid frontmatter")));
        }
        let old_links = note
            .links
            .iter()
            .enumerate()
            .filter(|(i, _)| !change.removed_links.contains(&(note.path.clone(), *i)))
            .map(|(_, l)| l)
            .collect::<Vec<_>>();
        // Edits replace link targets only, so links keep their number and order; pairing
        // by position is sound exactly when the counts match.
        let new_links = &new_note.links;
        if old_links.len() != new_links.len() {
            return Err(Refusal::refused(
                code,
                format!("{new_path} would have {} links instead of {}", new_links.len(), old_links.len()),
            ));
        }
        for (old_link, new_link) in old_links.into_iter().zip(new_links.iter()) {
            // Targets only: fragments are lint's concern (one into a merged note may dangle).
            let Some(before) = index.link_target(note, old_link) else { continue };
            let expected = targets.get(&before).cloned().unwrap_or(before);
            if after.link_target(new_note, new_link).as_ref() != Some(&expected) {
                return Err(Refusal::refused(
                    code,
                    format!("`{}` in {new_path} would no longer resolve to {expected}", new_link.raw),
                ));
            }
        }
    }
    Ok(())
}

/// Applies a change: renames, writes via temp file + rename, then deletions.
pub fn apply(vault: &Vault, change: &Change) -> std::io::Result<()> {
    for (old, new) in &change.moves {
        let dest = vault.abs(new);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(vault.abs(old), &dest)?;
    }
    // Writes before deletions: a failed write leaves the absorbed note in place.
    for (path, written) in &change.writes {
        vault::write_atomic(&vault.abs(path), written.text.as_bytes())?;
    }
    for path in &change.deleted {
        std::fs::remove_file(vault.abs(path))?;
    }
    // Remove directories left empty.
    for old in change.moves.keys().chain(&change.deleted) {
        let mut dir = old.parent();
        while let Some(d) = dir {
            if std::fs::remove_dir(vault.abs(&d)).is_err() {
                break;
            }
            dir = d.parent();
        }
    }
    Ok(())
}
