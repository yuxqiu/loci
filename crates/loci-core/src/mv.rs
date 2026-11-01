//! `loci mv` (m0 §9.5): move or rename files and directories, rewriting links.

use std::collections::{BTreeMap, BTreeSet};

use crate::envelope::Refusal;
use crate::index::Index;
use crate::relink::{self, Change};
use crate::vault::{self, Kind};

/// Computes the move of `from` to `to` without writing. Both are vault paths
/// ([`vault::path_arg`]), `from` as stored ([`stored_name`]). `ignored` tells whether a vault path is gitignored, and `exists` whether
/// it exists on disk (including files the index does not hold).
pub fn plan(
    index: &Index,
    from: &str,
    to: &str,
    ignored: impl Fn(&str) -> Result<bool, Refusal>,
    exists: impl Fn(&str) -> bool,
) -> Result<Change, Refusal> {
    if !vault::is_core_path(from) {
        return Err(Refusal::refused("mv.from", format!("`{from}` cannot be moved")));
    }
    if !vault::is_core_path(to) {
        return Err(Refusal::refused("mv.to", format!("`{to}` is not a valid destination")));
    }
    if ignored(to)? {
        return Err(Refusal::refused("path.ignored", format!("`{to}` is ignored by git")));
    }
    let is_file = index.files.binary_search(&from.to_string()).is_ok();
    let prefix = format!("{from}/");
    let dir_files: Vec<&String> = index.files.iter().filter(|f| f.starts_with(&prefix)).collect();
    if !is_file && dir_files.is_empty() {
        return Err(Refusal::refused("mv.from", format!("`{from}` does not exist or is ignored")));
    }
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    if is_file {
        map.insert(from.to_string(), to.to_string());
    } else {
        for f in dir_files {
            map.insert(f.clone(), format!("{to}/{}", &f[prefix.len()..]));
        }
    }
    // A source can be renamed, keeping its extension, but not moved out of `sources/`.
    if let Some((_, new)) =
        map.iter().find(|(old, new)| vault::source_ext(old).is_some_and(|ext| vault::source_ext(new) != Some(ext)))
    {
        return Err(Refusal::refused(
            "mv.source-file",
            format!("`{new}` is not a `sources/<type>/<slug>.<ext>` file with the same extension"),
        ));
    }
    // A file stays what it is: a note keeps its `.md`, and another file does not gain
    // one (it would be read as a note that is not one).
    if let Some((old, new)) = map.iter().find(|(old, new)| old.ends_with(".md") != new.ends_with(".md")) {
        let what = if old.ends_with(".md") {
            "a note keeps the `.md` extension"
        } else {
            "only a note has the `.md` extension"
        };
        return Err(Refusal::refused("mv.to", format!("`{new}`: {what}")));
    }
    // A concept stays a concept (as `new` checks before the path rules).
    if let Some((_, new)) = map
        .iter()
        .find(|(old, new)| vault::kind_of(old) == Some(Kind::Concept) && vault::kind_of(new) != Some(Kind::Concept))
    {
        return Err(Refusal::refused("mv.no-area", format!("`{new}` {}", vault::why_not_concept(new))));
    }
    // Every destination follows the path rules lint checks (`vault::path_problem`),
    // first, so the case advice below never leads to a name lint rejects.
    if let Some((new, (code, message))) = map.values().find_map(|n| vault::path_problem(n).map(|p| (n, p))) {
        return Err(Refusal::refused(code, format!("`{new}`: {message}")));
    }
    // A destination that differs from an existing name only in letter case (the source
    // itself, or a folder on its path) cannot be used safely: on a case-insensitive file
    // system (macOS) it is the existing file or folder, and git with `core.ignorecase`
    // keeps the existing spelling, so rewritten links would dangle; on other systems it
    // makes twin folders.
    if let Some((existing, asked)) = index.case_twin(to) {
        // `git mv -f` fits only a case-only rename of the source itself; for anyone
        // else's name it would make the twin, or on macOS overwrite the other file.
        let what_to_do = if existing == from {
            "rename it with `git mv -f`".to_string()
        } else if from.starts_with(&format!("{existing}/")) {
            // The source's own folder: renaming one file would leave the rest behind.
            format!("move the whole folder `{existing}` instead")
        } else {
            format!("use the existing spelling `{existing}` or another name")
        };
        return Err(Refusal::refused(
            "path.case-twin",
            format!("`{asked}` differs only in letter case from the existing `{existing}`; {what_to_do}"),
        ));
    }
    if exists(to) || index.has_entry(to) {
        return Err(Refusal::refused("mv.exists", format!("`{to}` already exists")));
    }
    // A moved file must stay in the vault: a tracked file matched by an ignore rule
    // would become an untracked, ignored one.
    for new in map.values() {
        if ignored(new)? {
            return Err(Refusal::refused("path.ignored", format!("`{new}` would be ignored by git")));
        }
    }
    // A destination that exists on disk (a file the index does not hold) is never
    // overwritten.
    if let Some(new) = map.values().find(|n| exists(n)) {
        return Err(Refusal::refused("mv.exists", format!("`{new}` already exists")));
    }
    // Every file that ends as a concept needs a slug that collides neither with a
    // concept that stays nor with another moved file.
    let mut new_slugs = BTreeSet::new();
    for new in map.values().filter(|n| vault::kind_of(n) == Some(Kind::Concept)) {
        let new_slug = vault::stem(new);
        if index.name_taken(new_slug, |p| map.contains_key(p)) || !new_slugs.insert(new_slug.to_lowercase()) {
            return Err(Refusal::refused("mv.collision", format!("slug `{new_slug}` is already used")));
        }
    }

    let mut change = Change { moves: map.clone(), ..Change::default() };
    let edits = relink::link_edits(index, &map, &BTreeSet::new());
    relink::add_writes(index, &mut change, edits)?;
    relink::verify(index, &map, &change, "mv.verify")?;
    Ok(change)
}

/// The stored spelling of `from`: itself when a file or folder has that name, else the
/// one stored file or folder whose name has `from` as its NFC form (`path.not-nfc`), so
/// that `mv` can fix it. Two stored spellings are ambiguous.
pub fn stored_name(index: &Index, from: &str) -> Result<String, Refusal> {
    let prefix = format!("{from}/");
    if index.has_entry(from) {
        return Ok(from.to_string());
    }
    let depth = from.split('/').count();
    let spellings: BTreeSet<String> = index
        .files
        .iter()
        .filter(|f| {
            let n = vault::nfc(f);
            n == from || n.starts_with(&prefix)
        })
        .map(|f| f.split('/').take(depth).collect::<Vec<_>>().join("/"))
        .collect();
    match spellings.len() {
        0 => Ok(from.to_string()),
        1 => Ok(spellings.into_iter().next().expect("one spelling")),
        _ => Err(Refusal::refused("mv.from", format!("`{from}` names several stored spellings; rename one with git"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::from_texts;

    const C: &str = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n---\n# T\n";

    #[test]
    fn rename_rewrites_links_and_leaves_the_note() {
        let i =
            from_texts(&[("concepts/a/old.md", C), ("views/v.md", "# V\n\n[[old]] [[old#T|x]] [[concepts/a/old]]\n")]);
        let p = plan(&i, "concepts/a/old.md", "concepts/a/new.md", |_| Ok(false), |_| false).unwrap();
        assert_eq!(p.writes["views/v.md"], "# V\n\n[[new]] [[new#T|x]] [[concepts/a/new]]\n");
        assert!(!p.writes.contains_key("concepts/a/new.md"), "a rename does not edit the note");
    }

    #[test]
    fn a_link_with_a_dangling_fragment_still_follows_its_note() {
        let i =
            from_texts(&[("concepts/a/dfs.md", C), ("views/w.md", "[[dfs#Typo]] [[concepts/a/dfs#^nope]] [[dfs]]\n")]);
        let p = plan(&i, "concepts/a/dfs.md", "concepts/a/depth-first.md", |_| Ok(false), |_| false).unwrap();
        assert_eq!(p.writes["views/w.md"], "[[depth-first#Typo]] [[concepts/a/depth-first#^nope]] [[depth-first]]\n");
    }

    #[test]
    fn rename_back_is_allowed() {
        let i = from_texts(&[("concepts/a/new.md", C), ("views/v.md", "[[new]]\n")]);
        let p = plan(&i, "concepts/a/new.md", "concepts/a/old.md", |_| Ok(false), |_| false).unwrap();
        assert_eq!(p.writes["views/v.md"], "[[old]]\n");
    }

    #[test]
    fn every_moved_file_must_stay_in_the_vault() {
        let i =
            from_texts(&[("views/d/fig.png", ""), ("views/d/x.md", "# X\n"), ("views/p.md", "[[views/d/fig.png]]\n")]);
        let png_ignored = |p: &str| Ok(p.ends_with(".png"));
        let e = plan(&i, "views/d", "views/e", png_ignored, |_| false).unwrap_err();
        assert_eq!(e.code(), "path.ignored");
    }

    #[test]
    fn from_is_found_by_its_nfc_form() {
        let nfd = "views/cafe\u{301}.md";
        let i = from_texts(&[(nfd, "# NFD\n"), ("views/de\u{301}/x.md", "# X\n")]);
        assert_eq!(stored_name(&i, "views/caf\u{e9}.md").unwrap(), nfd);
        // A folder too.
        assert_eq!(stored_name(&i, "views/d\u{e9}").unwrap(), "views/de\u{301}");
        // An exact name wins over another spelling.
        let both = from_texts(&[(nfd, "# NFD\n"), ("views/caf\u{e9}.md", "# NFC\n")]);
        assert_eq!(stored_name(&both, "views/caf\u{e9}.md").unwrap(), "views/caf\u{e9}.md");
        // Two stored spellings of one NFC name are ambiguous.
        let two = from_texts(&[("views/a\u{323}\u{302}.md", ""), ("views/a\u{302}\u{323}.md", "")]);
        assert_eq!(stored_name(&two, "views/\u{1ead}.md").unwrap_err().code(), "mv.from");
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
        let e = |f: &str, t: &str| plan(&i, f, t, |_| Ok(false), |_| false).unwrap_err().code().to_string();
        assert_eq!(e("concepts/a/x.md", "concepts/a/y.md"), "mv.exists");
        assert_eq!(e("views/v.md", "views/V.md"), "path.case-twin");
        // A case-only rename to a name lint rejects is refused for the name, not the case.
        assert_eq!(e("concepts/a/x.md", "concepts/a/X.md"), "slug.format");
        // Every path rule lint checks applies to a destination, the journal one included.
        let j = from_texts(&[("journal/research/e.md", "# E\n")]);
        let je = |f: &str, t: &str| plan(&j, f, t, |_| Ok(false), |_| false).unwrap_err().code().to_string();
        assert_eq!(je("journal/research", "journal/Research"), "journal.path");
        assert_eq!(je("journal/research/e.md", "notes.md"), "kind.unknown");
        assert_eq!(je("journal/research/e.md", "journal/My Stuff/e.md"), "journal.path");
        // A name that exists exactly is `mv.exists`, never twin advice.
        let twins = from_texts(&[("views/a.md", "# a\n"), ("views/A.md", "# A\n")]);
        let e2 = plan(&twins, "views/a.md", "views/A.md", |_| Ok(false), |_| false).unwrap_err();
        assert_eq!(e2.code(), "mv.exists");
        let message = |f: &str, t: &str| plan(&i, f, t, |_| Ok(false), |_| false).unwrap_err().error.message;
        assert!(message("views/v.md", "views/V.md").ends_with("rename it with `git mv -f`"));
        assert!(message("concepts/a/x.md", "concepts/A/z.md").ends_with("move the whole folder `concepts/a` instead"));
        assert!(
            message("views/v.md", "concepts/A/z.md")
                .ends_with("use the existing spelling `concepts/a` or another name")
        );
        assert_eq!(e("concepts/a", "concepts/A"), "path.case-twin", "a folder too");
        // A twin that breaks a path rule is refused for the rule (`Views/` is no known folder).
        assert_eq!(e("views", "Views"), "kind.unknown");
        assert_eq!(e("concepts/a/x.md", "concepts/A/z.md"), "path.case-twin", "a folder on the way");
        assert_eq!(e("concepts/a/x.md", "concepts/b/y.md"), "mv.collision");
        assert_eq!(e("concepts/a/x.md", "views/x.md"), "mv.no-area");
        assert_eq!(e("concepts/a/x.md", "concepts/x.md"), "mv.no-area", "the `concepts/` root is no area");
        // Messages say what is wrong with the destination.
        let message = |f: &str, t: &str| plan(&i, f, t, |_| Ok(false), |_| false).unwrap_err().error.message;
        assert!(message("concepts/a/x.md", "concepts/a/AREA.md").ends_with("would be the area file, not a concept"));
        assert_eq!(e("views/v.md", "views/w"), "mv.to");
        let pic = from_texts(&[("views/fig.png", ""), ("views/v.md", "[[views/fig.png]]\n")]);
        let pe = plan(&pic, "views/fig.png", "views/fig.md", |_| Ok(false), |_| false).unwrap_err();
        assert_eq!(
            (pe.code(), pe.error.message.as_str()),
            ("mv.to", "`views/fig.md`: only a note has the `.md` extension")
        );
        assert!(message("views/v.md", "views/w").ends_with("a note keeps the `.md` extension"));
        let folder = from_texts(&[("views/D/x.md", "# X\n"), ("views/D/y.md", "# Y\n")]);
        let m = plan(&folder, "views/D/x.md", "views/d/x.md", |_| Ok(false), |_| false).unwrap_err().error.message;
        assert!(m.ends_with("move the whole folder `views/D` instead"), "{m}");
        assert_eq!(e("sources/papers/p.pdf", "sources/papers/p/q.pdf"), "mv.source-file");
        assert_eq!(e("sources/papers/p.pdf", "views/p.pdf"), "mv.source-file");
        assert_eq!(e("sources/papers/p.pdf", "sources/papers/p.md"), "mv.source-file", "a source keeps its extension");
        // A stray file under `sources/` may leave, which fixes its `sources.layout`.
        assert!(plan(&i, "sources/stray.md", "views/stray.md", |_| Ok(false), |_| false).is_ok());
        assert_eq!(e("plugins/learn", "x"), "mv.from");
        assert_eq!(e("loci.toml", "views/x.toml"), "mv.from", "configuration cannot be moved");
        assert_eq!(e("sources", "old/sources"), "mv.source-file");
        assert_eq!(e("concepts/a/x.md", "concepts/a/Bad Name.md"), "slug.format");
        // A file that becomes a concept follows the concept naming rules too.
        assert_eq!(e("views/v.md", "concepts/b/x.md"), "mv.collision");
        assert_eq!(e("views/v.md", "concepts/b/Bad.md"), "slug.format");
        // Two moved files may not end with the same concept slug.
        let drafts = from_texts(&[
            ("concepts/a/AREA.md", "# A\n"),
            ("inbox/drafts/x.md", "# X\n"),
            ("inbox/drafts/sub/x.md", "# X2\n"),
        ]);
        let e = plan(&drafts, "inbox/drafts", "concepts/a/new", |_| Ok(false), |_| false).unwrap_err();
        assert_eq!(e.code(), "mv.collision");
        assert!(plan(&i, "sources/papers/p.pdf", "sources/papers/q.pdf", |_| Ok(false), |_| false).is_ok());
        assert!(plan(&i, "sources/papers", "sources/articles", |_| Ok(false), |_| false).is_ok());
    }

    #[test]
    fn source_citations_follow() {
        let c = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n---\n# T\n\nSee [[sources/papers/p.pdf#p3]].\n";
        let i = from_texts(&[("concepts/a/x.md", c), ("sources/papers/p.pdf", "")]);
        let p = plan(&i, "sources/papers/p.pdf", "sources/papers/q.pdf", |_| Ok(false), |_| false).unwrap();
        assert!(p.writes["concepts/a/x.md"].contains("[[sources/papers/q.pdf#p3]]"));
    }

    #[test]
    fn table_pipe_kept() {
        let i = from_texts(&[("concepts/a/old.md", C), ("views/v.md", "# V\n\n| a |\n|---|\n| [[old\\|x]] |\n")]);
        let p = plan(&i, "concepts/a/old.md", "concepts/a/new.md", |_| Ok(false), |_| false).unwrap();
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
        let p = plan(&i, "concepts/a/g", "concepts/a/h", |_| Ok(false), |_| false).unwrap();
        assert_eq!(p.moves.len(), 2);
        assert_eq!(p.writes["views/v.md"], "# V\n\n[[x]] ![[concepts/a/h/p.png]]\n");
    }
}
