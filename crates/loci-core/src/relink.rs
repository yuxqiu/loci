//! Link-preserving changes shared by `mv` and `merge`: rewrite links whose targets move,
//! check the result in memory, and apply it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::envelope::Refusal;
use crate::index::{self, Index, Resolved, SourceTarget};
use crate::parse::{Link, LinkForm, Note};
use crate::splice::{self, Edit};
use crate::vault::{self, Kind};

/// A computed change to the vault.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Change {
    /// Files renamed on disk: old path -> new path.
    pub moves: BTreeMap<String, String>,
    /// Files deleted from disk.
    pub deleted: BTreeSet<String>,
    /// New content of every edited file, keyed by its final path.
    pub writes: BTreeMap<String, String>,
    /// Number of edits per edited file (final path).
    pub edit_counts: BTreeMap<String, usize>,
    /// Links removed on purpose, as (original path, index into that note's `links`).
    pub removed_links: BTreeSet<(String, usize)>,
}

impl Change {
    fn final_path(&self, path: &str) -> String {
        self.moves.get(path).cloned().unwrap_or_else(|| path.to_string())
    }
}

/// Builds the edits for every link, in files outside `sources/` and not in `skip`, whose
/// resolved target is a key of `targets` (old target path -> new target path).
pub fn link_edits(
    index: &Index,
    targets: &BTreeMap<String, String>,
    skip: &BTreeSet<String>,
) -> BTreeMap<String, Vec<Edit>> {
    let mut edits: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
    for note in index.notes.values().filter(|n| n.kind != Kind::Source && !skip.contains(&n.path)) {
        for link in &note.links {
            if let Some(new_raw) = rewrite(index, note, link, targets) {
                edits.entry(note.path.clone()).or_default().push(Edit::replace(link.target_range.clone(), new_raw));
            }
        }
    }
    edits
}

/// The new raw target for a link whose target moves, keeping its form.
fn rewrite(index: &Index, note: &Note, link: &Link, targets: &BTreeMap<String, String>) -> Option<String> {
    if link.form == LinkForm::Source {
        return rewrite_source(index, link, targets);
    }
    let resolved = index.resolve(note, link).ok()?;
    let new = targets.get(resolved.path())?;
    if link.target.is_empty() {
        return None; // `[[#frag]]` follows its own note.
    }
    let mut raw = if !link.target.contains('/') {
        vault::stem(new).to_string()
    } else {
        match resolved {
            Resolved::Note(_) => new.strip_suffix(".md").unwrap_or(new).to_string(),
            Resolved::File(_) => new.clone(),
        }
    };
    if link.form != LinkForm::Prereq {
        if let Some(f) = &link.fragment {
            raw.push('#');
            raw.push_str(f);
        }
        if link.raw.ends_with('\\') {
            raw.push('\\');
        }
    }
    (raw != link.raw).then_some(raw)
}

/// The new raw value of a `sources` entry whose journal note or source folder moves.
fn rewrite_source(index: &Index, link: &Link, targets: &BTreeMap<String, String>) -> Option<String> {
    let suffix = link.fragment.as_ref().map(|f| format!("#{f}")).unwrap_or_default();
    let new = source_destination(&index.resolve_source(link)?, targets)??;
    Some(format!("{}{suffix}", new.strip_suffix(".md").unwrap_or(&new)))
}

/// Where a `sources` target goes: `None` if it does not move, `Some(None)` if it moves
/// somewhere a `sources` entry cannot name. A folder moves as a whole: its new path is
/// the folder of any moved file.
fn source_destination(target: &SourceTarget, targets: &BTreeMap<String, String>) -> Option<Option<String>> {
    match target {
        SourceTarget::Note(note) => targets.get(note).map(|new| Some(new.clone())),
        SourceTarget::Folder(folder) => {
            let new =
                targets.iter().find(|(old, _)| vault::source_folder(old).is_some_and(|(f, _)| f == folder.path))?.1;
            Some(vault::source_folder(new).map(|(f, _)| f.to_string()))
        }
    }
}

/// The edit that removes one item of a frontmatter list (flow or block form).
pub fn remove_list_item(
    note: &Note,
    list_span: std::ops::Range<usize>,
    item: std::ops::Range<usize>,
    flow: bool,
) -> Edit {
    let text = &note.text;
    if flow {
        // Remove the item and one adjacent comma with its spaces.
        let after = &text[item.end..];
        let trailing = after.len() - after.trim_start_matches([',', ' ']).len();
        if after.starts_with(',') || after.trim_start().starts_with(',') {
            return Edit::replace(item.start..item.end + trailing, "");
        }
        let before = &text[list_span.start..item.start];
        let leading = before.len() - before.trim_end_matches([',', ' ']).len();
        return Edit::replace(item.start - leading..item.end, "");
    }
    remove_lines(text, item)
}

/// The edit that removes the whole lines spanned by `range`.
pub fn remove_lines(text: &str, range: std::ops::Range<usize>) -> Edit {
    let line_start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[range.end..].find('\n').map_or(text.len(), |i| range.end + i + 1);
    Edit::replace(line_start..line_end, "")
}

/// Applies per-file edits into `change.writes`.
pub fn add_writes(index: &Index, change: &mut Change, edits: BTreeMap<String, Vec<Edit>>) -> Result<(), Refusal> {
    for (path, file_edits) in edits {
        let note = &index.notes[&path];
        let count = file_edits.len();
        let text =
            splice::apply(&note.text, file_edits).map_err(|e| Refusal::internal("change.overlap", e.to_string()))?;
        let dest = change.final_path(&path);
        change.edit_counts.insert(dest.clone(), count);
        change.writes.insert(dest, text);
    }
    Ok(())
}

/// Builds the changed tree in memory and checks that every link outside `sources/` that
/// resolved before resolves to `targets`' image of its old target (m0 §9.5 step 4).
/// A fragment into a deleted note (the absorbed note of a merge) may stop resolving.
pub fn verify(
    index: &Index,
    targets: &BTreeMap<String, String>,
    change: &Change,
    code: &'static str,
) -> Result<(), Refusal> {
    let mut files: Vec<String> =
        index.files.iter().filter(|f| !change.deleted.contains(*f)).map(|f| change.final_path(f)).collect();
    files.sort();
    let contents: BTreeMap<String, Vec<u8>> = index
        .notes
        .values()
        .filter(|n| !change.deleted.contains(&n.path))
        .map(|n| {
            let p = change.final_path(&n.path);
            let text = change.writes.get(&p).cloned().unwrap_or_else(|| n.text.clone());
            (p, text.into_bytes())
        })
        .collect();
    let after = index::build(files, |p| {
        contents.get(p).cloned().ok_or_else(|| std::io::Error::other("not a readable note before the change"))
    });
    for note in index.notes.values().filter(|n| n.kind != Kind::Source && !change.deleted.contains(&n.path)) {
        let new_path = change.final_path(&note.path);
        let Some(new_note) = after.notes.get(&new_path) else {
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
            .filter(|l| l.form != LinkForm::Source)
            .collect::<Vec<_>>();
        let new_links: Vec<&Link> = new_note.links.iter().filter(|l| l.form != LinkForm::Source).collect();
        // Edits replace link targets only, so links keep their number and order; pairing
        // by position is sound exactly when the counts match.
        if old_links.len() != new_links.len() {
            return Err(Refusal::refused(
                code,
                format!("{new_path} would have {} links instead of {}", new_links.len(), old_links.len()),
            ));
        }
        let old_sources = note.links.iter().filter(|l| l.form == LinkForm::Source);
        let new_sources: Vec<&Link> = new_note.links.iter().filter(|l| l.form == LinkForm::Source).collect();
        for (old_link, new_link) in old_sources.zip(new_sources) {
            let Some(before) = index.resolve_source(old_link) else { continue };
            let expected = match source_destination(&before, targets) {
                None => Some(before.into_path()),
                Some(new) => new,
            };
            let after_path = after.resolve_source(new_link).map(SourceTarget::into_path);
            if expected.is_none() || after_path != expected {
                return Err(Refusal::refused(
                    code,
                    format!("source `{}` in {new_path} would no longer resolve", new_link.raw),
                ));
            }
        }
        for (old_link, new_link) in old_links.into_iter().zip(new_links) {
            let Ok(before) = index.resolve(note, old_link) else { continue };
            let expected = targets.get(before.path()).cloned().unwrap_or_else(|| before.path().to_string());
            match after.resolve(new_note, new_link) {
                Ok(r) if r.path() == expected => {}
                // A fragment into the absorbed note of a merge may now dangle; lint
                // reports it. A move deletes no note, so it never may.
                Err(index::LinkError::Heading | index::LinkError::Block) if change.deleted.contains(before.path()) => {}
                _ => {
                    return Err(Refusal::refused(
                        code,
                        format!("`{}` in {new_path} would no longer resolve to {expected}", new_link.raw),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Applies a change: renames, writes via temp file + rename, then deletions.
pub fn apply(root: &Path, change: &Change) -> std::io::Result<()> {
    for (old, new) in &change.moves {
        let dest = root.join(new);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(root.join(old), &dest)?;
    }
    // Writes before deletions: a failed write leaves the absorbed note in place.
    for (path, text) in &change.writes {
        vault::write_atomic(&root.join(path), text.as_bytes())?;
    }
    for path in &change.deleted {
        std::fs::remove_file(root.join(path))?;
    }
    // Remove directories left empty.
    for old in change.moves.keys().chain(&change.deleted) {
        let mut dir = Path::new(old).parent();
        while let Some(d) = dir.filter(|d| !d.as_os_str().is_empty()) {
            if std::fs::remove_dir(root.join(d)).is_err() {
                break;
            }
            dir = d.parent();
        }
    }
    Ok(())
}
