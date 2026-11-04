//! Link-preserving changes shared by `mv` and `merge`: rewrite links whose targets move,
//! check the result in memory, and apply it.

use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::path::Path;

use crate::envelope::Refusal;
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
    pub writes: BTreeMap<VaultPath, Edited>,
}

/// The new content of an edited file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    pub text: String,
    /// How many edits made it.
    pub edits: usize,
}

/// Links removed on purpose, as (original path, index into that note's `links`).
type Removed = BTreeSet<(VaultPath, usize)>;

impl Change {
    pub(crate) fn final_path(&self, path: &VaultPath) -> VaultPath {
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

        for (note, note_edits) in edits {
            let count = note_edits.len();
            let text = splice::apply(&note.text, note_edits).map_err(|e| Refusal::internal("change.edit", e))?;
            change.writes.insert(change.final_path(&note.path), Edited { text, edits: count });
        }

        verify(index, &targets, &change, &removed, code)?;
        Ok(change)
    }

    /// Applies a change: writes staged, renames, writes put in place, then deletions.
    /// Every write is staged in its final folder before anything changes, so one that
    /// cannot be made (a read-only folder) fails with the vault untouched; a rename that
    /// fails undoes the earlier ones. A symlink on the way to a written or moved file is
    /// refused first (`file.symlink`).
    pub fn apply(&self, vault: &Vault) -> Result<(), Refusal> {
        for path in self.writes.keys().chain(self.moves.values()) {
            vault.refuse_symlink(path)?;
        }
        let mut staged = vec![];
        for (path, edited) in &self.writes {
            let dest = vault.abs(path);
            let dir = dest.parent().expect("a vault file has a folder");
            std::fs::create_dir_all(dir).map_err(at(dir))?;
            staged.push((vault::stage(&dest, edited.text.as_bytes()).map_err(at(&dest))?, dest));
        }

        let mut renamed = vec![];
        for (old, new) in &self.moves {
            if let Err(failed) = rename(vault, old, new) {
                drop(staged);
                let undone = renamed.into_iter().rev().try_for_each(|(old, new)| rename(vault, new, old));
                let made = self.moves.values().chain(self.writes.keys());
                let cleaned = undone.and_then(|()| remove_empty_folders(vault, made));
                return Err(match cleaned {
                    Ok(()) => failed,
                    Err(e) => {
                        Refusal::io(format!("{}; undoing the earlier moves: {}", failed.error.message, e.error.message))
                    }
                });
            }
            renamed.push((old, new));
        }

        // Writes before deletions: a failed write leaves the absorbed note in place.
        for (tmp, dest) in staged {
            vault::persist(tmp, &dest).map_err(at(&dest))?;
        }

        for path in &self.deleted {
            let path = vault.abs(path);
            std::fs::remove_file(&path).map_err(at(&path))?;
        }
        remove_empty_folders(vault, self.moves.keys().chain(&self.deleted))
    }
}

/// Renames vault file `old` to `new`, creating `new`'s folders.
fn rename(vault: &Vault, old: &VaultPath, new: &VaultPath) -> Result<(), Refusal> {
    let (from, to) = (vault.abs(old), vault.abs(new));
    let dir = to.parent().expect("a vault file has a folder");
    std::fs::create_dir_all(dir).map_err(at(dir))?;
    std::fs::rename(&from, &to)
        .map_err(|e| Refusal::io(format!("renaming {} -> {}: {e}", from.display(), to.display())))
}

/// Removes the folders of `paths` left empty, each up to the first that is not.
fn remove_empty_folders<'p>(vault: &Vault, paths: impl Iterator<Item = &'p VaultPath>) -> Result<(), Refusal> {
    for path in paths {
        let mut dir = path.parent();
        while let Some(d) = dir {
            let abs = vault.abs(&d);
            match std::fs::remove_dir(&abs) {
                Ok(()) => dir = d.parent(),
                Err(e) if matches!(e.kind(), ErrorKind::DirectoryNotEmpty | ErrorKind::NotFound) => break,
                Err(e) => return Err(at(&abs)(e)),
            }
        }
    }
    Ok(())
}

/// The edits for every link, in files outside `sources/` that `change` does not
/// delete, whose resolved target is a key of `targets` (old target path -> new target
/// path). Prereqs are rewritten with their note's frontmatter (m0 §6.1); one that would
/// name its own note (the absorbing note of a merge) is removed, and returned.
fn link_edits<'i>(
    index: &'i Index,
    targets: &BTreeMap<VaultPath, VaultPath>,
    change: &Change,
) -> (Vec<(&'i Note, Vec<Edit>)>, Removed) {
    // Written targets name files in the tree after the move.
    let tree = index.vault().after(change);
    let mut edits = vec![];
    let mut removed = Removed::new();
    for note in relinked(index, change) {
        let (mut note_edits, mut prereqs) = (vec![], vec![]);
        for (i, link) in note.links.iter().enumerate() {
            let Some(new) = index.link_target(note, link).and_then(|t| targets.get(t)) else { continue };
            let Some(slug) = link.prereq_slug() else {
                if let Some(written) = rewrite(&tree, note, link, new) {
                    note_edits.push(Edit::replace(link.target.span.clone(), written));
                }
                continue;
            };

            // A prereq names its concept by slug; one moved to a name that is no slug
            // cannot be named, so the prereq stays and verification refuses the change.
            let from = slug.clone();
            if *new == change.final_path(&note.path) {
                prereqs.push(FmChange::RenamePrereq { from, to: None });
                removed.insert((note.path.clone(), i));
            } else if let Some(to) = new.slug().filter(|to| to != slug) {
                prereqs.push(FmChange::RenamePrereq { from, to: Some(to) });
            }
        }

        if !prereqs.is_empty() {
            note_edits.push(note.rewrite_frontmatter(&prereqs));
        }
        if !note_edits.is_empty() {
            edits.push((note, note_edits));
        }
    }

    (edits, removed)
}

/// The notes whose links a change rewrites and checks: outside `sources/`, and not
/// deleted by it.
fn relinked<'i>(index: &'i Index, change: &Change) -> impl Iterator<Item = &'i Note> {
    index.notes().filter(|n| n.kind != Kind::Source && !change.deleted.contains(&n.path))
}

/// The new written target for a body link whose target moves to `new` in `tree`, named
/// the same way (a slug stays a slug, a path a path) with its fragment kept. A link into
/// its own note follows the note. The fragment is not checked: one that no longer
/// resolves still follows its note (lint reports it).
fn rewrite(tree: &Vault, note: &Note, link: &Spanned<Link>, new: &VaultPath) -> Option<String> {
    let mut written = match &link.target.names {
        Names::ThisNote => return None,
        // `[[b.md]]` stays as written when its concept keeps its slug.
        Names::Slug(slug) if new.stem() == slug.as_str() => return None,
        Names::Slug(_) => new.stem().to_string(),
        Names::Path(_) => written(tree, new),
        Names::Bare(_) | Names::Escaping(_) => unreachable!("it resolves to nothing, so it never moves"),
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
    let tree = index.vault().after(change);
    let after = Index::open(&tree);
    for note in relinked(index, change) {
        verify_note(index, &after, targets, change, removed, note).map_err(|why| Refusal::refused(code, why))?;
    }
    Ok(())
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
        let expected = targets.get(before).unwrap_or(before);
        if after.link_target(new_note, new_link) != Some(expected) {
            let written = new_link.target.text(&new_note.text);
            return Err(format!("`{written}` in {new_path} would no longer resolve to {expected}"));
        }
    }
    Ok(())
}

/// The shortest written path that resolves to `path` in `vault` ([`Vault::resolve`]):
/// a note's without `.md`, unless that names another file. It keeps a `/` (a path
/// without one is a bare name, m0 §5.3): a root file keeps its full path, which names
/// nothing, so verification refuses a change that moves a linked file to the root.
pub(crate) fn written(vault: &Vault, path: &VaultPath) -> String {
    match path.as_str().strip_suffix(".md") {
        Some(short) if path.parent().is_some() && vault.resolve(short) == Some(path) => short.to_string(),
        _ => path.to_string(),
    }
}

/// An io refusal naming the path it was about, for `map_err`.
fn at(path: &Path) -> impl Fn(std::io::Error) -> Refusal + '_ {
    move |e| Refusal::io(format!("{}: {e}", path.display()))
}
