//! `loci mv` (m0 §9.5): move or rename files and directories, rewriting links.

use std::collections::{BTreeMap, BTreeSet};

use crate::envelope::Refusal;
use crate::index::Index;
use crate::relink::{self, Change};
use crate::vault::{self, Kind};

/// Computes the move of `from` to `to` without writing. Both are vault paths
/// ([`vault::path_arg`]). `to_ignored` tells whether `to` is gitignored and `exists`
/// whether a vault path exists on disk (including files the index does not hold).
pub fn plan(
    index: &Index,
    from: &str,
    to: &str,
    to_ignored: bool,
    exists: impl Fn(&str) -> bool,
) -> Result<Change, Refusal> {
    if from.starts_with("plugins/") || from == "plugins" {
        return Err(Refusal::refused("mv.from", format!("`{from}` cannot be moved")));
    }
    if to.starts_with("plugins/") || to == "plugins" || vault::is_reserved(to) || to_ignored {
        return Err(Refusal::refused("mv.to", format!("`{to}` is not a valid destination")));
    }
    let is_file = index.files.binary_search(&from.to_string()).is_ok();
    let prefix = format!("{from}/");
    let dir_files: Vec<&String> = index.files.iter().filter(|f| f.starts_with(&prefix)).collect();
    if !is_file && dir_files.is_empty() {
        return Err(Refusal::refused("mv.from", format!("`{from}` does not exist or is ignored")));
    }
    if exists(to) || index.files.iter().any(|f| f == to || f.starts_with(&format!("{to}/"))) {
        return Err(Refusal::refused("mv.exists", format!("`{to}` already exists")));
    }
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    if is_file {
        map.insert(from.to_string(), to.to_string());
    } else {
        for f in dir_files {
            map.insert(f.clone(), format!("{to}/{}", &f[prefix.len()..]));
        }
    }
    // A source can be renamed, keeping its type (extension), but not moved out of
    // `sources/<type>/<slug>.<ext>`; any other file moved under `sources/` must become
    // a source. A stray file (`sources.layout`) may move anywhere.
    let layout_kept = |old: &str, new: &str| match vault::source_ext(old) {
        Some(ext) => vault::source_ext(new) == Some(ext),
        None => !new.starts_with("sources/") || vault::is_source_file(new),
    };
    if let Some((_, new)) = map.iter().find(|(old, new)| !layout_kept(old, new)) {
        return Err(Refusal::refused(
            "mv.source-file",
            format!("`{new}` is not a `sources/<type>/<slug>.<ext>` file of the same type"),
        ));
    }
    // A destination that exists on disk (a file the index does not hold) is never
    // overwritten.
    if let Some(new) = map.values().find(|n| exists(n)) {
        return Err(Refusal::refused("mv.exists", format!("`{new}` already exists")));
    }
    // Moved concepts must stay concepts; every file that ends as a concept needs a valid
    // slug that does not collide.
    for (old, new) in &map {
        let was_concept = vault::kind_of(old) == Some(Kind::Concept);
        if vault::kind_of(new) != Some(Kind::Concept) {
            if was_concept {
                return Err(Refusal::refused("mv.no-area", format!("`{new}` is not inside an area")));
            }
            continue;
        }
        let new_slug = vault::stem(new);
        if index.name_taken(new_slug, |p| map.contains_key(p)) {
            return Err(Refusal::refused("mv.collision", format!("slug `{new_slug}` is already used")));
        }
        if !vault::is_slug(new_slug) {
            return Err(Refusal::refused("slug.format", format!("`{new_slug}` is not a valid slug")));
        }
    }

    let mut change = Change { moves: map.clone(), ..Change::default() };
    let edits = relink::link_edits(index, &map, &BTreeSet::new());
    relink::add_writes(index, &mut change, edits)?;
    relink::verify(index, &map, &change, "mv.verify")?;
    Ok(change)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::from_texts;

    const C: &str = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\ndigest: d\n---\n# T\n";

    #[test]
    fn rename_rewrites_links_and_leaves_the_note() {
        let i =
            from_texts(&[("concepts/a/old.md", C), ("views/v.md", "# V\n\n[[old]] [[old#T|x]] [[concepts/a/old]]\n")]);
        let p = plan(&i, "concepts/a/old.md", "concepts/a/new.md", false, |_| false).unwrap();
        assert_eq!(p.writes["views/v.md"], "# V\n\n[[new]] [[new#T|x]] [[concepts/a/new]]\n");
        assert!(!p.writes.contains_key("concepts/a/new.md"), "a rename does not edit the note");
    }

    #[test]
    fn rename_back_is_allowed() {
        let i = from_texts(&[("concepts/a/new.md", C), ("views/v.md", "[[new]]\n")]);
        let p = plan(&i, "concepts/a/new.md", "concepts/a/old.md", false, |_| false).unwrap();
        assert_eq!(p.writes["views/v.md"], "[[old]]\n");
    }

    #[test]
    fn an_unreadable_concept_moves_unchanged() {
        let i = crate::index::build(vec!["concepts/a/bad.md".to_string()], |_| Ok(vec![0xff]));
        let p = plan(&i, "concepts/a/bad.md", "concepts/a/good.md", false, |_| false).unwrap();
        assert!(p.writes.is_empty());
    }

    #[test]
    fn collisions_and_refusals() {
        let i = from_texts(&[
            ("concepts/a/x.md", C),
            ("concepts/a/y.md", C),
            ("concepts/b/AREA.md", "# B\n"),
            ("views/v.md", "# V\n"),
            ("sources/papers/p.pdf", ""),
            ("sources/stray.md", "# Stray\n"),
        ]);
        let e = |f: &str, t: &str| plan(&i, f, t, false, |_| false).unwrap_err().code().to_string();
        assert_eq!(e("concepts/a/x.md", "concepts/a/y.md"), "mv.exists");
        assert_eq!(e("concepts/a/x.md", "concepts/b/y.md"), "mv.collision");
        assert_eq!(e("concepts/a/x.md", "views/x.md"), "mv.no-area");
        assert_eq!(e("sources/papers/p.pdf", "sources/papers/p/q.pdf"), "mv.source-file");
        assert_eq!(e("sources/papers/p.pdf", "views/p.pdf"), "mv.source-file");
        assert_eq!(e("sources/papers/p.pdf", "sources/papers/p.md"), "mv.source-file", "a source keeps its type");
        // A stray file under `sources/` may leave, which fixes its `sources.layout`.
        assert!(plan(&i, "sources/stray.md", "views/stray.md", false, |_| false).is_ok());
        assert_eq!(e("plugins/learn", "x"), "mv.from");
        assert_eq!(e("sources", "old/sources"), "mv.source-file");
        assert_eq!(e("concepts/a/x.md", "concepts/a/Bad Name.md"), "slug.format");
        // A file that becomes a concept follows the concept naming rules too.
        assert_eq!(e("views/v.md", "concepts/b/x.md"), "mv.collision");
        assert_eq!(e("views/v.md", "concepts/b/Bad.md"), "slug.format");
        assert!(plan(&i, "sources/papers/p.pdf", "sources/papers/q.pdf", false, |_| false).is_ok());
        assert!(plan(&i, "sources/papers", "sources/articles", false, |_| false).is_ok());
    }

    #[test]
    fn source_citations_follow() {
        let c = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\ndigest: d\n---\n# T\n\nSee [[sources/papers/p.pdf#p3]].\n";
        let i = from_texts(&[("concepts/a/x.md", c), ("sources/papers/p.pdf", "")]);
        let p = plan(&i, "sources/papers/p.pdf", "sources/papers/q.pdf", false, |_| false).unwrap();
        assert!(p.writes["concepts/a/x.md"].contains("[[sources/papers/q.pdf#p3]]"));
    }

    #[test]
    fn table_pipe_kept() {
        let i = from_texts(&[("concepts/a/old.md", C), ("views/v.md", "# V\n\n| a |\n|---|\n| [[old\\|x]] |\n")]);
        let p = plan(&i, "concepts/a/old.md", "concepts/a/new.md", false, |_| false).unwrap();
        assert!(p.writes["views/v.md"].contains("[[new\\|x]]"));
    }

    #[test]
    fn directory_move() {
        let i = from_texts(&[
            ("concepts/a/AREA.md", "# A\n\nP.\n"),
            ("concepts/a/g/x.md", C),
            ("views/v.md", "# V\n\n[[x]] ![[concepts/a/g/p.png]]\n"),
            ("concepts/a/g/p.png", ""),
        ]);
        let p = plan(&i, "concepts/a/g", "concepts/a/h", false, |_| false).unwrap();
        assert_eq!(p.moves.len(), 2);
        assert_eq!(p.writes["views/v.md"], "# V\n\n[[x]] ![[concepts/a/h/p.png]]\n");
    }
}
