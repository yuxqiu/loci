//! `loci mv` (m0 §9.5): move or rename files and directories, rewriting links.

use std::collections::{BTreeMap, BTreeSet};

use crate::index::Index;
use crate::relink::{self, Change, ChangeError, err};
use crate::vault::{self, Kind};

/// Computes the move of `from` to `to` without writing. Both are vault paths
/// ([`vault::user_path`]). `to_ignored` tells whether `to` is gitignored and `exists`
/// whether a vault path exists on disk (including files the index does not hold).
pub fn plan(
    index: &Index,
    from: &str,
    to: &str,
    to_ignored: bool,
    exists: impl Fn(&str) -> bool,
) -> Result<Change, ChangeError> {
    if from.starts_with("plugins/") || from == "plugins" {
        return Err(err("mv.from", format!("`{from}` cannot be moved")));
    }
    if to.starts_with("plugins/") || to == "plugins" || vault::is_reserved(to) || to_ignored {
        return Err(err("mv.to", format!("`{to}` is not a valid destination")));
    }
    let is_file = index.files.binary_search(&from.to_string()).is_ok();
    let prefix = format!("{from}/");
    let dir_files: Vec<&String> = index.files.iter().filter(|f| f.starts_with(&prefix)).collect();
    if !is_file && dir_files.is_empty() {
        return Err(err("mv.from", format!("`{from}` does not exist or is ignored")));
    }
    if exists(to) || index.files.iter().any(|f| f == to || f.starts_with(&format!("{to}/"))) {
        return Err(err("mv.exists", format!("`{to}` already exists")));
    }
    // Sources move only as whole folders.
    if from.starts_with("sources/") || to.starts_with("sources/") {
        let whole_folder = !is_file
            && from.starts_with("sources/")
            && to.starts_with("sources/")
            && from.split('/').count() == 3
            && to.split('/').count() == 3;
        if !whole_folder {
            return Err(err("mv.source-file", "only a whole source folder `sources/<type>/<slug>` can move"));
        }
    }
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    if is_file {
        map.insert(from.to_string(), to.to_string());
    } else {
        for f in dir_files {
            map.insert(f.clone(), format!("{to}/{}", &f[prefix.len()..]));
        }
    }
    // A destination that exists on disk (a file the index does not hold) is never
    // overwritten.
    if let Some(new) = map.values().find(|n| exists(n)) {
        return Err(err("mv.exists", format!("`{new}` already exists")));
    }
    // Moved concepts must stay concepts; new slugs must not collide.
    let mut renames: Vec<(String, String)> = vec![]; // (old path, old slug)
    for (old, new) in &map {
        if vault::kind_of(old) != Some(Kind::Concept) {
            continue;
        }
        if vault::kind_of(new) != Some(Kind::Concept) {
            return Err(err("mv.no-area", format!("`{new}` is not inside an area")));
        }
        let (old_slug, new_slug) = (vault::stem(old), vault::stem(new));
        let key = new_slug.to_lowercase();
        let taken_by_other = index.slugs.get(&key).is_some_and(|ps| ps.iter().any(|p| !map.contains_key(p)));
        if taken_by_other || index.aliases.contains_key(&key) {
            return Err(err("mv.collision", format!("slug `{new_slug}` is already a slug or alias")));
        }
        if old_slug != new_slug {
            renames.push((old.clone(), old_slug.to_string()));
        }
    }

    let mut change = Change { moves: map.clone(), ..Change::default() };
    let mut edits = relink::link_edits(index, &map, &BTreeSet::new());
    for (old_path, old_slug) in &renames {
        let note = &index.notes[old_path];
        let edit = relink::alias_edit(note, std::slice::from_ref(old_slug), "mv.frontmatter")?;
        edits.entry(old_path.clone()).or_default().push(edit);
        change.aliases_added.push((map[old_path].clone(), old_slug.clone()));
    }
    relink::add_writes(index, &mut change, edits)?;
    relink::verify(index, &map, &change, "mv.verify", false)?;
    Ok(change)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(files: &[(&str, &str)]) -> Index {
        crate::index::from_texts(files)
    }

    const C: &str = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\ndigest: d\n---\n# T\n";

    #[test]
    fn rename_rewrites_bare_and_adds_alias() {
        let i = idx(&[("concepts/a/old.md", C), ("views/v.md", "# V\n\n[[old]] [[old#T|x]] [[concepts/a/old]]\n")]);
        let p = plan(&i, "concepts/a/old.md", "concepts/a/new.md", false, |_| false).unwrap();
        assert_eq!(p.writes["views/v.md"], "# V\n\n[[new]] [[new#T|x]] [[concepts/a/new]]\n");
        assert!(p.writes["concepts/a/new.md"].contains("aliases: [old]\n"));
        assert_eq!(p.aliases_added, vec![("concepts/a/new.md".into(), "old".into())]);
    }

    #[test]
    fn alias_appended_to_flow_and_block_lists() {
        let flow = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\naliases: [x]\ndigest: d\n---\n# T\n";
        let i = idx(&[("concepts/a/old.md", flow)]);
        let p = plan(&i, "concepts/a/old.md", "concepts/a/new.md", false, |_| false).unwrap();
        assert!(p.writes["concepts/a/new.md"].contains("aliases: [x, old]\n"));
        let block = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\naliases:\n  - x\ndigest: d\n---\n# T\n";
        let i = idx(&[("concepts/a/old.md", block)]);
        let p = plan(&i, "concepts/a/old.md", "concepts/a/new.md", false, |_| false).unwrap();
        assert!(p.writes["concepts/a/new.md"].contains("aliases:\n  - x\n  - old\n"));
    }

    #[test]
    fn alias_after_trailing_comma_and_in_multiline_flow_lists() {
        let fm =
            |aliases: &str| format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\naliases: {aliases}\ndigest: d\n---\n# T\n");
        for (before, after) in [("[x, ]", "[x, old, ]"), ("[x,\n  y\n]", "[x,\n  y, old\n]"), ("[ ]", "[ old]")] {
            let text = fm(before);
            let i = idx(&[("concepts/a/old.md", &text)]);
            let p = plan(&i, "concepts/a/old.md", "concepts/a/new.md", false, |_| false).unwrap();
            assert!(p.writes["concepts/a/new.md"].contains(&format!("aliases: {after}\n")), "{before:?}");
        }
    }

    #[test]
    fn crlf_alias_line() {
        let crlf = "---\r\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\r\ndigest: d\r\n---\r\n# T\r\n";
        let i = idx(&[("concepts/a/old.md", crlf)]);
        let p = plan(&i, "concepts/a/old.md", "concepts/a/new.md", false, |_| false).unwrap();
        assert!(p.writes["concepts/a/new.md"].contains("TV\r\naliases: [old]\r\ndigest"));
    }

    #[test]
    fn collisions_and_refusals() {
        let with_alias = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\naliases: [taken]\ndigest: d\n---\n# T\n";
        let i = idx(&[
            ("concepts/a/x.md", C),
            ("concepts/a/y.md", with_alias),
            ("sources/papers/p/paper.pdf", ""),
            ("sources/papers/p/source.yaml", "sha256: x\n"),
        ]);
        let e = |f: &str, t: &str| plan(&i, f, t, false, |_| false).unwrap_err().code;
        assert_eq!(e("concepts/a/x.md", "concepts/a/y.md"), "mv.exists");
        assert_eq!(e("concepts/a/x.md", "concepts/a/taken.md"), "mv.collision");
        assert_eq!(e("concepts/a/x.md", "views/x.md"), "mv.no-area");
        assert_eq!(e("sources/papers/p/paper.pdf", "sources/papers/q.pdf"), "mv.source-file");
        assert_eq!(e("plugins/learn", "x"), "mv.from");
        assert!(plan(&i, "sources/papers/p", "sources/papers/q", false, |_| false).is_ok());
    }

    #[test]
    fn source_folder_citations_follow() {
        let c = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\nsources: [\"sources/papers/p#p3\"]\ndigest: d\n---\n# T\n";
        let i = idx(&[
            ("concepts/a/x.md", c),
            ("sources/papers/p/paper.pdf", ""),
            ("sources/papers/p/source.yaml", "sha256: x\n"),
        ]);
        let p = plan(&i, "sources/papers/p", "sources/papers/q", false, |_| false).unwrap();
        assert!(p.writes["concepts/a/x.md"].contains("\"sources/papers/q#p3\""));
    }

    #[test]
    fn table_pipe_kept() {
        let i = idx(&[("concepts/a/old.md", C), ("views/v.md", "# V\n\n| a |\n|---|\n| [[old\\|x]] |\n")]);
        let p = plan(&i, "concepts/a/old.md", "concepts/a/new.md", false, |_| false).unwrap();
        assert!(p.writes["views/v.md"].contains("[[new\\|x]]"));
    }

    #[test]
    fn directory_move() {
        let i = idx(&[
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
