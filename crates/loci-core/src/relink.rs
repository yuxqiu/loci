//! Link-preserving changes shared by `mv` and `merge`: rewrite links whose targets move,
//! check the result in memory, and apply it.

use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;

use crate::envelope::Refusal;
use crate::id::Slug;
use crate::index::Index;
use crate::parse::{FmChange, Link, Names, Note, Spanned};
use crate::path::Kind;
use crate::path::VaultPath;
use crate::splice::{self, Edit};
use crate::vault::{self, Vault};

/// A computed change to the vault.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Change {
    /// Files renamed on disk: old path -> new path.
    pub moves: BTreeMap<VaultPath, VaultPath>,
    /// Files deleted from disk.
    pub deleted: BTreeSet<VaultPath>,
    /// Every edited file, keyed by its final path.
    pub writes: BTreeMap<VaultPath, Written>,
}

/// The new content of an edited file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub text: String,
    /// How many edits made it.
    pub edits: usize,
}

/// Links removed on purpose, as (original path, index into that note's `links`).
type Removed = BTreeSet<(VaultPath, usize)>;

impl Change {
    fn final_path(&self, path: &VaultPath) -> VaultPath {
        self.moves.get(path).unwrap_or(path).clone()
    }

    /// Plans a link-preserving change (m0 §9.5): files `moves` (old path -> new path), and
    /// notes absorbed into others, deleted (`absorbed`: deleted path -> the note that takes
    /// its links). Every link to a moved or absorbed file is rewritten to follow it, then
    /// the changed tree is checked in memory, refusing with `code` if a link would break.
    pub fn relink(
        index: &Index,
        moves: BTreeMap<VaultPath, VaultPath>,
        absorbed: BTreeMap<VaultPath, VaultPath>,
        code: &'static str,
    ) -> Result<Change, Refusal> {
        let targets: BTreeMap<VaultPath, VaultPath> = moves.clone().into_iter().chain(absorbed.clone()).collect();
        let mut change = Change { moves, deleted: absorbed.into_keys().collect(), ..Change::default() };
        let (edits, removed) = link_edits(index, &targets, &change);

        for (path, file_edits) in edits {
            let note = index.parsed(&path).expect("edits are only made to readable notes");
            let count = file_edits.len();
            let text = splice::apply(&note.text, file_edits).map_err(|e| Refusal::internal("change.edit", e))?;
            change.writes.insert(change.final_path(&path), Written { text, edits: count });
        }

        verify(index, &targets, &change, &removed, code)?;
        Ok(change)
    }

    /// Applies a change: renames, writes via temp file + rename, then deletions.
    pub fn apply(&self, vault: &Vault) -> std::io::Result<()> {
        for (old, new) in &self.moves {
            let dest = vault.abs(new);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(vault.abs(old), &dest)?;
        }

        // Writes before deletions: a failed write leaves the absorbed note in place.
        for (path, written) in &self.writes {
            vault::write_atomic(&vault.abs(path), written.text.as_bytes())?;
        }

        for path in &self.deleted {
            std::fs::remove_file(vault.abs(path))?;
        }

        // Remove directories left empty, up to the first that is not.
        for old in self.moves.keys().chain(&self.deleted) {
            let mut dir = old.parent();
            while let Some(d) = dir {
                match std::fs::remove_dir(vault.abs(&d)) {
                    Ok(()) => dir = d.parent(),
                    Err(e) if matches!(e.kind(), ErrorKind::DirectoryNotEmpty | ErrorKind::NotFound) => break,
                    Err(e) => return Err(e),
                }
            }
        }

        Ok(())
    }
}

/// The edits for every link, in files outside `sources/` that `change` does not
/// delete, whose resolved target is a key of `targets` (old target path -> new target
/// path). Prereqs are rewritten with their note's frontmatter (m0 §6.1); one that would
/// name its own note (the absorbing note of a merge) is removed, and returned.
fn link_edits(
    index: &Index,
    targets: &BTreeMap<VaultPath, VaultPath>,
    change: &Change,
) -> (BTreeMap<VaultPath, Vec<Edit>>, Removed) {
    let mut edits: BTreeMap<VaultPath, Vec<Edit>> = BTreeMap::new();
    let mut removed = Removed::new();
    for note in index.notes().filter(|n| n.kind != Kind::Source && !change.deleted.contains(&n.path)) {
        let mut prereqs = vec![];
        for (i, link) in note.links.iter().enumerate() {
            let Some(new) = index.link_target(note, link).and_then(|t| targets.get(&t)) else { continue };
            let Some(slug) = link.prereq_slug() else {
                if let Some(written) = rewrite(note, link, new) {
                    edits.entry(note.path.clone()).or_default().push(Edit::replace(link.target.span.clone(), written));
                }
                continue;
            };

            // A prereq names its concept by slug; one moved to a name that is no slug
            // cannot be named, so the prereq stays and verification refuses the change.
            let from = slug.clone();
            if *new == change.final_path(&note.path) {
                prereqs.push(FmChange::RenamePrereq { from, to: None });
                removed.insert((note.path.clone(), i));
            } else if let Some(to) = new.stem().parse::<Slug>().ok().filter(|to| to != slug) {
                prereqs.push(FmChange::RenamePrereq { from, to: Some(to) });
            }
        }

        if !prereqs.is_empty() {
            let edit = note.rewrite_frontmatter(&prereqs);
            edits.entry(note.path.clone()).or_default().push(edit);
        }
    }

    (edits, removed)
}

/// The new written target for a body link whose target moves to `new`, named the same
/// way (a slug stays a slug, a path a path) with its fragment kept. A link into its own
/// note follows the note. The fragment is not checked: one that no longer resolves
/// still follows its note (lint reports it).
fn rewrite(note: &Note, link: &Spanned<Link>, new: &VaultPath) -> Option<String> {
    let mut written = match &link.target.names {
        Names::ThisNote => return None,
        Names::Slug(_) | Names::Bare(_) => new.stem().to_string(),
        // A note is linked without `.md`, any other file with its extension.
        Names::Path(_) => new.as_str().strip_suffix(".md").unwrap_or(new.as_str()).to_string(),
        Names::Escaping(_) => unreachable!("an escaping path resolves to nothing, so it never moves"),
    };
    if let Some(f) = &link.target.fragment {
        written.push_str(&format!("#{f}"));
    }
    (written != link.target.text(&note.text)).then_some(written)
}

/// Builds the changed tree in memory and checks that every link outside `sources/` that
/// resolved before, but for the `removed` ones, resolves to `targets`' image of its old
/// target (m0 §9.5 step 3), refusing with `code` when one does not.
fn verify(
    index: &Index,
    targets: &BTreeMap<VaultPath, VaultPath>,
    change: &Change,
    removed: &Removed,
    code: &'static str,
) -> Result<(), Refusal> {
    let after = rebuilt(index, change);
    for note in index.notes().filter(|n| n.kind != Kind::Source && !change.deleted.contains(&n.path)) {
        verify_note(index, &after, targets, change, removed, note).map_err(|why| Refusal::refused(code, why))?;
    }
    Ok(())
}

/// The index of the tree `change` would leave: its moves and deletions done, its writes
/// in place.
fn rebuilt(index: &Index, change: &Change) -> Index {
    let mut files: Vec<VaultPath> =
        index.files().iter().filter(|f| !change.deleted.contains(*f)).map(|f| change.final_path(f)).collect();
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
    Index::build(files, |p| contents[p].clone())
}

/// Why `note`'s links would break in `after`, if they would. Links are paired by
/// position, which holds because edits only rewrite targets in place (`fix` checks its
/// own, link-adding edits differently), so the counts must match. Targets only:
/// fragments are lint's concern, and one into a merge's absorbed note may dangle.
fn verify_note(
    index: &Index,
    after: &Index,
    targets: &BTreeMap<VaultPath, VaultPath>,
    change: &Change,
    removed: &Removed,
    note: &Note,
) -> Result<(), String> {
    let new_path = change.final_path(&note.path);
    let Some(new_note) = after.parsed(&new_path) else {
        return Err(format!("{new_path} would no longer parse as a note"));
    };
    if !note.has_broken_frontmatter() && new_note.has_broken_frontmatter() {
        return Err(format!("{new_path} would get invalid frontmatter"));
    }

    let kept = |(i, _): &(usize, &Spanned<Link>)| !removed.contains(&(note.path.clone(), *i));
    let old_links: Vec<&Spanned<Link>> = note.links.iter().enumerate().filter(kept).map(|(_, l)| l).collect();
    if old_links.len() != new_note.links.len() {
        return Err(format!("{new_path} would have {} links instead of {}", new_note.links.len(), old_links.len()));
    }

    for (old_link, new_link) in old_links.into_iter().zip(&new_note.links) {
        let Some(before) = index.link_target(note, old_link) else { continue };
        let expected = targets.get(&before).cloned().unwrap_or(before);
        if after.link_target(new_note, new_link).as_ref() != Some(&expected) {
            let written = new_link.target.text(&new_note.text);
            return Err(format!("`{written}` in {new_path} would no longer resolve to {expected}"));
        }
    }
    Ok(())
}
