//! Link-preserving changes shared by `mv` and `merge`: rewrite links whose targets move,
//! edit `aliases`, check the result in memory, and apply it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::index::{self, Index, Resolved};
use crate::parse::{FmValue, Link, LinkForm, Note};
use crate::splice::{self, Edit};
use crate::vault::{self, Kind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeError {
    pub code: &'static str,
    pub message: String,
}

pub fn err(code: &'static str, message: impl Into<String>) -> ChangeError {
    ChangeError { code, message: message.into() }
}

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
    /// (concept's final path, alias added).
    pub aliases_added: Vec<(String, String)>,
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
        return rewrite_source(link, targets);
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

fn rewrite_source(link: &Link, targets: &BTreeMap<String, String>) -> Option<String> {
    let target = &link.target;
    let suffix = link.fragment.as_ref().map(|f| format!("#{f}")).unwrap_or_default();
    if let Some(new) = targets.get(&format!("{target}.md")) {
        return Some(format!("{}{suffix}", new.strip_suffix(".md").unwrap_or(new)));
    }
    if target.starts_with("sources/") {
        let prefix = format!("{target}/");
        let (old, new) = targets.iter().find(|(o, _)| o.starts_with(&prefix))?;
        let rest = &old[prefix.len()..];
        let new_folder = new.strip_suffix(&format!("/{rest}"))?;
        return Some(format!("{new_folder}{suffix}"));
    }
    None
}

/// The frontmatter edit that appends `aliases` to a concept's `aliases` (m0 §9.5 step 3).
/// Refuses with `code` if the frontmatter is missing or invalid, or `aliases` is not a
/// list.
pub fn alias_edit(note: &Note, aliases: &[String], code: &'static str) -> Result<Edit, ChangeError> {
    let text = &note.text;
    let refuse = |why: &str| Err(err(code, format!("cannot add aliases to `{}`: {why}", note.path)));
    let Some(fm) = note.frontmatter.as_ref().filter(|_| note.fm_error.is_none()) else {
        return refuse("it has no valid frontmatter");
    };
    Ok(match fm.get("aliases").map(|e| &e.value) {
        // After the last item, so a trailing comma or a multi-line list stays valid.
        Some(FmValue::List { items, flow: true, .. }) if !items.is_empty() => {
            let last = items.last().expect("checked non-empty").span();
            Edit::insert(last.end, format!(", {}", aliases.join(", ")))
        }
        Some(FmValue::List { span, flow: true, .. }) => {
            // saphyr's flow-sequence span ends before the `]` (m0 §4.4).
            let close = span.end + text[span.end..].find(']').expect("a parsed flow list closes with `]`");
            Edit::insert(close, aliases.join(", "))
        }
        // A block list is never empty: an empty `aliases:` is null.
        Some(FmValue::List { items, .. }) => {
            let last = items.last().expect("checked non-empty").span();
            let line_start = text[..last.start].rfind('\n').map_or(0, |i| i + 1);
            let indent = &text[line_start..last.start];
            let line_end = text[last.end..].find('\n').map_or(text.len(), |i| last.end + i + 1);
            let eol = line_ending(text, line_end);
            Edit::insert(line_end, aliases.iter().map(|a| format!("{indent}{a}{eol}")).collect::<String>())
        }
        Some(_) => return refuse("`aliases` is not a list"),
        None => {
            // No `aliases`: insert a line after `id:` (or at the start of the YAML).
            let anchor = fm.get("id").map(|e| e.value.span().end).unwrap_or(fm.yaml_range.start);
            let line_end = text[anchor..].find('\n').map_or(text.len(), |i| anchor + i + 1);
            let eol = line_ending(text, line_end);
            Edit::insert(line_end, format!("aliases: [{}]{eol}", aliases.join(", ")))
        }
    })
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

/// The line ending of the line that ends at `line_end` (exclusive).
fn line_ending(text: &str, line_end: usize) -> &'static str {
    if text[..line_end].ends_with("\r\n") { "\r\n" } else { "\n" }
}

/// Applies per-file edits into `change.writes`.
pub fn add_writes(index: &Index, change: &mut Change, edits: BTreeMap<String, Vec<Edit>>) -> Result<(), ChangeError> {
    for (path, file_edits) in edits {
        let note = &index.notes[&path];
        let count = file_edits.len();
        let text = splice::apply(&note.text, file_edits).map_err(|e| err("change.overlap", e.to_string()))?;
        let dest = change.final_path(&path);
        change.edit_counts.insert(dest.clone(), count);
        change.writes.insert(dest, text);
    }
    Ok(())
}

/// Builds the changed tree in memory and checks that every link outside `sources/` that
/// resolved before resolves to `targets`' image of its old target (m0 §9.5 step 4).
/// With `fragments_may_dangle` (merge), a fragment into a retargeted note may stop
/// resolving.
pub fn verify(
    index: &Index,
    targets: &BTreeMap<String, String>,
    change: &Change,
    code: &'static str,
    fragments_may_dangle: bool,
) -> Result<(), ChangeError> {
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
            return Err(err(code, format!("{new_path} would no longer parse as a note")));
        };
        if note.fm_error.is_none() && new_note.fm_error.is_some() {
            return Err(err(code, format!("{new_path} would get invalid frontmatter")));
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
            return Err(err(
                code,
                format!("{new_path} would have {} links instead of {}", new_links.len(), old_links.len()),
            ));
        }
        for (old_link, new_link) in old_links.into_iter().zip(new_links) {
            let Ok(before) = index.resolve(note, old_link) else { continue };
            let expected = targets.get(before.path()).cloned().unwrap_or_else(|| before.path().to_string());
            match after.resolve(new_note, new_link) {
                Ok(r) if r.path() == expected => {}
                // A fragment into the absorbed note of a merge may now dangle; lint
                // reports it. A move keeps note contents, so it never may.
                Err(index::LinkError::Heading | index::LinkError::Block)
                    if fragments_may_dangle && expected != before.path() => {}
                _ => {
                    return Err(err(
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
