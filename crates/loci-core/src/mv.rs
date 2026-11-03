//! `loci mv` (m0 §9.5): move or rename files and directories, rewriting links.

use std::collections::{BTreeMap, BTreeSet};

use crate::envelope::Refusal;
use crate::index::Index;
use crate::path::{Kind, VaultPath};
use crate::relink::Change;

/// Computes the move of `from` to `to` without writing: `from` as stored
/// ([`stored_name`]). `ignored` tells whether a vault path is gitignored, and `exists`
/// whether it exists on disk (including files the index does not hold). The checks run
/// in this order, so the first refusal is the most telling one; the command then adds
/// the two that read git listings (a folder moved in part, a changed source).
pub fn plan(
    index: &Index,
    from: &VaultPath,
    to: &VaultPath,
    ignored: impl Fn(&VaultPath) -> Result<bool, Refusal>,
    exists: impl Fn(&VaultPath) -> bool,
) -> Result<Change, Refusal> {
    refuse_ends(from, to, &ignored)?;
    let moves = moves(index, from, to)?;
    refuse_changed_kinds(&moves)?;
    refuse_unsafe_destination(index, from, to, &exists)?;
    refuse_lost_files(&moves, &ignored, &exists)?;
    refuse_slug_collisions(index, &moves)?;

    Change::relink(index, moves, BTreeMap::new(), "mv.verify")
}

/// `from` and `to` are paths core moves, and `to` is not ignored (checked before the
/// path rules, so a move into an ignored folder says so).
fn refuse_ends(
    from: &VaultPath,
    to: &VaultPath,
    ignored: &impl Fn(&VaultPath) -> Result<bool, Refusal>,
) -> Result<(), Refusal> {
    if !from.is_core() {
        return Err(Refusal::refused("mv.from", format!("`{from}` cannot be moved")));
    }
    if !to.is_core() {
        return Err(Refusal::refused("mv.to", format!("`{to}` is not a valid destination")));
    }
    if ignored(to)? {
        return Err(Refusal::refused("path.ignored", format!("`{to}` is ignored by git")));
    }
    Ok(())
}

/// Every file the move renames, old path -> new path: `from` itself, or every file
/// under folder `from`.
fn moves(index: &Index, from: &VaultPath, to: &VaultPath) -> Result<BTreeMap<VaultPath, VaultPath>, Refusal> {
    if index.contains(from) {
        return Ok(BTreeMap::from([(from.clone(), to.clone())]));
    }
    let moves: BTreeMap<VaultPath, VaultPath> = index
        .files()
        .iter()
        .filter(|f| f.starts_with_folder(from))
        .map(|f| (f.clone(), f.with_prefix_replaced(from, to).expect("`f` is inside `from`")))
        .collect();
    if moves.is_empty() {
        return Err(Refusal::refused("mv.from", format!("`{from}` does not exist or is ignored")));
    }
    Ok(moves)
}

/// Every moved file stays what it is, and its new path follows the path rules lint
/// checks (`VaultPath::problem`).
fn refuse_changed_kinds(moves: &BTreeMap<VaultPath, VaultPath>) -> Result<(), Refusal> {
    // A source can be renamed, keeping its extension, but not moved out of `sources/`.
    if let Some((_, new)) =
        moves.iter().find(|(old, new)| old.source_ext().is_some_and(|ext| new.source_ext() != Some(ext)))
    {
        return Err(Refusal::refused(
            "mv.source-file",
            format!("`{new}` is not a `sources/<type>/<slug>.<ext>` file with the same extension"),
        ));
    }

    // A note keeps its `.md`, and another file does not gain one (it would be read as a
    // note that is not one).
    if let Some((old, new)) = moves.iter().find(|(old, new)| old.is_note() != new.is_note()) {
        let what =
            if old.is_note() { "a note keeps the `.md` extension" } else { "only a note has the `.md` extension" };
        return Err(Refusal::refused("mv.to", format!("`{new}`: {what}")));
    }

    // A concept stays a concept (as `new` checks before the path rules).
    let concept = |p: &VaultPath| p.kind() == Some(Kind::Concept);
    if let Some((_, new)) = moves.iter().find(|(old, new)| concept(old) && !concept(new)) {
        return Err(Refusal::refused("mv.no-area", format!("`{new}` {}", new.why_not_concept())));
    }

    // The path rules come before the case advice below, so it never leads to a name
    // lint rejects.
    if let Some((new, problem)) = moves.values().find_map(|n| n.problem().map(|p| (n, p))) {
        return Err(Refusal::refused(problem.code(), format!("`{new}`: {problem}")));
    }

    Ok(())
}

/// `to` neither exists nor differs from an existing name only in letter case.
fn refuse_unsafe_destination(
    index: &Index,
    from: &VaultPath,
    to: &VaultPath,
    exists: &impl Fn(&VaultPath) -> bool,
) -> Result<(), Refusal> {
    refuse_case_twin(index, to, Some(from))?;
    if exists(to) || index.has_entry(to) {
        return Err(Refusal::refused("mv.exists", format!("`{to}` already exists")));
    }

    Ok(())
}

/// Refuses destination `to` when it differs only in letter case from an existing file or
/// folder on its path (`path.case-twin`), for `mv` (moving `source`) and `new` (no
/// source). Such a twin cannot be used safely: on a case-insensitive file system
/// (macOS) it is the existing file or folder, and git with `core.ignorecase` keeps the
/// existing spelling, so rewritten links would dangle; on other systems it makes twin
/// folders.
pub fn refuse_case_twin(index: &Index, to: &VaultPath, source: Option<&VaultPath>) -> Result<(), Refusal> {
    let Some((existing, asked)) = index.case_twin(to) else { return Ok(()) };
    // `git mv -f` fits only a case-only rename of the source itself; for anyone else's
    // name it would make the twin, or on macOS overwrite the other file.
    let what_to_do = match source {
        Some(from) if existing == *from => "rename it with `git mv -f`".to_string(),
        // The source's own folder: renaming one file would leave the rest behind.
        Some(from) if from.starts_with_folder(&existing) => format!("move the whole folder `{existing}` instead"),
        _ => format!("use the existing spelling `{existing}` or another name"),
    };
    Err(Refusal::refused(
        "path.case-twin",
        format!("`{asked}` differs only in letter case from the existing `{existing}`; {what_to_do}"),
    ))
}

/// No moved file is lost: none becomes ignored (a tracked file matched by an ignore rule
/// would become an untracked, ignored one), and none overwrites a file on disk the index
/// does not hold.
fn refuse_lost_files(
    moves: &BTreeMap<VaultPath, VaultPath>,
    ignored: &impl Fn(&VaultPath) -> Result<bool, Refusal>,
    exists: &impl Fn(&VaultPath) -> bool,
) -> Result<(), Refusal> {
    for new in moves.values() {
        if ignored(new)? {
            return Err(Refusal::refused("path.ignored", format!("`{new}` would be ignored by git")));
        }
    }
    if let Some(new) = moves.values().find(|n| exists(n)) {
        return Err(Refusal::refused("mv.exists", format!("`{new}` already exists")));
    }
    Ok(())
}

/// Every file that ends as a concept has a slug that collides neither with a concept
/// that stays nor with another moved file.
fn refuse_slug_collisions(index: &Index, moves: &BTreeMap<VaultPath, VaultPath>) -> Result<(), Refusal> {
    let mut new_slugs = BTreeSet::new();
    for new in moves.values().filter(|n| n.kind() == Some(Kind::Concept)) {
        let slug = new.slug().expect("`problem` refused a concept whose stem is no slug");
        let taken = index.slug_paths(&slug).iter().any(|p| !moves.contains_key(p));
        if taken || !new_slugs.insert(slug.clone()) {
            return Err(Refusal::refused("mv.collision", format!("slug `{slug}` is already used")));
        }
    }
    Ok(())
}

/// The stored spelling of `from`: itself when a file or folder has that name, else the
/// one stored file or folder whose name has `from` as its NFC form (`path.not-nfc`), so
/// that `mv` can fix it. Two stored spellings are ambiguous.
pub fn stored_name(index: &Index, from: &VaultPath) -> Result<VaultPath, Refusal> {
    if index.has_entry(from) {
        return Ok(from.clone());
    }
    let depth = from.components().count();
    let spellings: BTreeSet<VaultPath> = index
        .files()
        .iter()
        .filter(|f| {
            let n = VaultPath::new(crate::path::nfc(f.as_str())).expect("NFC keeps a vault path one");
            n == *from || n.starts_with_folder(from)
        })
        .map(|f| f.head(depth).expect("a file at or under `from` has as many segments"))
        .collect();
    match spellings.len() {
        0 => Ok(from.clone()),
        1 => Ok(spellings.into_iter().next().expect("one spelling")),
        _ => Err(Refusal::refused("mv.from", format!("`{from}` names several stored spellings; rename one with git"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Index;

    fn vp(s: &str) -> VaultPath {
        VaultPath::new(s.to_string()).unwrap()
    }

    const C: &str = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n---\n# T\n";

    #[test]
    fn rename_rewrites_links_and_leaves_the_note() {
        let i = Index::from_texts(&[
            ("concepts/a/old.md", C),
            ("views/v.md", "# V\n\n[[old]] [[old#T|x]] [[concepts/a/old]]\n"),
        ]);
        let p = plan(&i, &vp("concepts/a/old.md"), &vp("concepts/a/new.md"), |_| Ok(false), |_| false).unwrap();
        assert_eq!(p.writes["views/v.md"].text, "# V\n\n[[new]] [[new#T|x]] [[concepts/a/new]]\n");
        assert!(!p.writes.contains_key("concepts/a/new.md"), "a rename does not edit the note");
    }

    #[test]
    fn a_link_with_a_dangling_fragment_still_follows_its_note() {
        let i = Index::from_texts(&[
            ("concepts/a/dfs.md", C),
            ("views/w.md", "[[dfs#Typo]] [[concepts/a/dfs#^nope]] [[dfs]]\n"),
        ]);
        let p = plan(&i, &vp("concepts/a/dfs.md"), &vp("concepts/a/depth-first.md"), |_| Ok(false), |_| false).unwrap();
        assert_eq!(
            p.writes["views/w.md"].text,
            "[[depth-first#Typo]] [[concepts/a/depth-first#^nope]] [[depth-first]]\n"
        );
    }

    #[test]
    fn rename_back_is_allowed() {
        let i = Index::from_texts(&[("concepts/a/new.md", C), ("views/v.md", "[[new]]\n")]);
        let p = plan(&i, &vp("concepts/a/new.md"), &vp("concepts/a/old.md"), |_| Ok(false), |_| false).unwrap();
        assert_eq!(p.writes["views/v.md"].text, "[[old]]\n");
    }

    #[test]
    fn every_moved_file_must_stay_in_the_vault() {
        let i = Index::from_texts(&[
            ("views/d/fig.png", ""),
            ("views/d/x.md", "# X\n"),
            ("views/p.md", "[[views/d/fig.png]]\n"),
        ]);
        let png_ignored = |p: &VaultPath| Ok(p.as_str().ends_with(".png"));
        let e = plan(&i, &vp("views/d"), &vp("views/e"), png_ignored, |_| false).unwrap_err();
        assert_eq!(e.code(), "path.ignored");
    }

    #[test]
    fn from_is_found_by_its_nfc_form() {
        let nfd = "views/cafe\u{301}.md";
        let i = Index::from_texts(&[(nfd, "# NFD\n"), ("views/de\u{301}/x.md", "# X\n")]);
        assert_eq!(stored_name(&i, &vp("views/caf\u{e9}.md")).unwrap(), nfd);
        // A folder too.
        assert_eq!(stored_name(&i, &vp("views/d\u{e9}")).unwrap(), "views/de\u{301}");
        // An exact name wins over another spelling.
        let both = Index::from_texts(&[(nfd, "# NFD\n"), ("views/caf\u{e9}.md", "# NFC\n")]);
        assert_eq!(stored_name(&both, &vp("views/caf\u{e9}.md")).unwrap(), "views/caf\u{e9}.md");
        // Two stored spellings of one NFC name are ambiguous.
        let two = Index::from_texts(&[("views/a\u{323}\u{302}.md", ""), ("views/a\u{302}\u{323}.md", "")]);
        assert_eq!(stored_name(&two, &vp("views/\u{1ead}.md")).unwrap_err().code(), "mv.from");
    }

    #[test]
    fn collisions_and_refusals() {
        let i = Index::from_texts(&[
            ("concepts/a/x.md", C),
            ("concepts/a/y.md", C),
            ("concepts/b/AREA.md", "# B\n"),
            ("views/v.md", "# V\n"),
            ("sources/papers/p.pdf", ""),
            ("sources/stray.md", "# Stray\n"),
        ]);

        let e = |f: &str, t: &str| plan(&i, &vp(f), &vp(t), |_| Ok(false), |_| false).unwrap_err().code().to_string();
        assert_eq!(e("concepts/a/x.md", "concepts/a/y.md"), "mv.exists");
        assert_eq!(e("views/v.md", "views/V.md"), "path.case-twin");

        // A case-only rename to a name lint rejects is refused for the name, not the case.
        assert_eq!(e("concepts/a/x.md", "concepts/a/X.md"), "slug.format");

        // Every path rule lint checks applies to a destination, the journal one included.
        let j = Index::from_texts(&[("journal/research/e.md", "# E\n")]);
        let je = |f: &str, t: &str| plan(&j, &vp(f), &vp(t), |_| Ok(false), |_| false).unwrap_err().code().to_string();
        assert_eq!(je("journal/research", "journal/Research"), "journal.path");
        assert_eq!(je("journal/research/e.md", "notes.md"), "kind.unknown");
        assert_eq!(je("journal/research/e.md", "journal/My Stuff/e.md"), "journal.path");

        // A name that exists exactly is `mv.exists`, never twin advice.
        let twins = Index::from_texts(&[("views/a.md", "# a\n"), ("views/A.md", "# A\n")]);
        let e2 = plan(&twins, &vp("views/a.md"), &vp("views/A.md"), |_| Ok(false), |_| false).unwrap_err();
        assert_eq!(e2.code(), "mv.exists");
        let message = |f: &str, t: &str| plan(&i, &vp(f), &vp(t), |_| Ok(false), |_| false).unwrap_err().error.message;
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
        let message = |f: &str, t: &str| plan(&i, &vp(f), &vp(t), |_| Ok(false), |_| false).unwrap_err().error.message;
        assert!(message("concepts/a/x.md", "concepts/a/AREA.md").ends_with("would be the area file, not a concept"));
        assert_eq!(e("views/v.md", "views/w"), "mv.to");
        let pic = Index::from_texts(&[("views/fig.png", ""), ("views/v.md", "[[views/fig.png]]\n")]);
        let pe = plan(&pic, &vp("views/fig.png"), &vp("views/fig.md"), |_| Ok(false), |_| false).unwrap_err();
        assert_eq!(
            (pe.code(), pe.error.message.as_str()),
            ("mv.to", "`views/fig.md`: only a note has the `.md` extension")
        );

        assert!(message("views/v.md", "views/w").ends_with("a note keeps the `.md` extension"));
        let folder = Index::from_texts(&[("views/D/x.md", "# X\n"), ("views/D/y.md", "# Y\n")]);
        let m = plan(&folder, &vp("views/D/x.md"), &vp("views/d/x.md"), |_| Ok(false), |_| false)
            .unwrap_err()
            .error
            .message;
        assert!(m.ends_with("move the whole folder `views/D` instead"), "{m}");
        assert_eq!(e("sources/papers/p.pdf", "sources/papers/p/q.pdf"), "mv.source-file");
        assert_eq!(e("sources/papers/p.pdf", "views/p.pdf"), "mv.source-file");
        assert_eq!(e("sources/papers/p.pdf", "sources/papers/p.md"), "mv.source-file", "a source keeps its extension");

        // A stray file under `sources/` may leave, which fixes its `sources.layout`.
        assert!(plan(&i, &vp("sources/stray.md"), &vp("views/stray.md"), |_| Ok(false), |_| false).is_ok());
        assert_eq!(e("plugins/learn", "x"), "mv.from");
        assert_eq!(e("loci.toml", "views/x.toml"), "mv.from", "configuration cannot be moved");
        assert_eq!(e("sources", "old/sources"), "mv.source-file");
        assert_eq!(e("concepts/a/x.md", "concepts/a/Bad Name.md"), "slug.format");

        // A file that becomes a concept follows the concept naming rules too.
        assert_eq!(e("views/v.md", "concepts/b/x.md"), "mv.collision");
        assert_eq!(e("views/v.md", "concepts/b/Bad.md"), "slug.format");

        // Two moved files may not end with the same concept slug.
        let drafts = Index::from_texts(&[
            ("concepts/a/AREA.md", "# A\n"),
            ("inbox/drafts/x.md", "# X\n"),
            ("inbox/drafts/sub/x.md", "# X2\n"),
        ]);

        let e = plan(&drafts, &vp("inbox/drafts"), &vp("concepts/a/new"), |_| Ok(false), |_| false).unwrap_err();
        assert_eq!(e.code(), "mv.collision");
        assert!(plan(&i, &vp("sources/papers/p.pdf"), &vp("sources/papers/q.pdf"), |_| Ok(false), |_| false).is_ok());
        assert!(plan(&i, &vp("sources/papers"), &vp("sources/articles"), |_| Ok(false), |_| false).is_ok());
    }

    #[test]
    fn source_citations_follow() {
        let c = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n---\n# T\n\nSee [[sources/papers/p.pdf#p3]].\n";
        let i = Index::from_texts(&[("concepts/a/x.md", c), ("sources/papers/p.pdf", "")]);
        let p = plan(&i, &vp("sources/papers/p.pdf"), &vp("sources/papers/q.pdf"), |_| Ok(false), |_| false).unwrap();
        assert!(p.writes["concepts/a/x.md"].text.contains("[[sources/papers/q.pdf#p3]]"));
    }

    #[test]
    fn table_pipe_kept() {
        let i =
            Index::from_texts(&[("concepts/a/old.md", C), ("views/v.md", "# V\n\n| a |\n|---|\n| [[old\\|x]] |\n")]);
        let p = plan(&i, &vp("concepts/a/old.md"), &vp("concepts/a/new.md"), |_| Ok(false), |_| false).unwrap();
        assert!(p.writes["views/v.md"].text.contains("[[new\\|x]]"));
    }

    #[test]
    fn directory_move() {
        let i = Index::from_texts(&[
            ("concepts/a/AREA.md", "# A\n\nP.\n"),
            ("concepts/a/g/x.md", C),
            ("views/v.md", "# V\n\n[[x]] ![[concepts/a/g/p.png]]\n"),
            ("concepts/a/g/p.png", ""),
        ]);
        let p = plan(&i, &vp("concepts/a/g"), &vp("concepts/a/h"), |_| Ok(false), |_| false).unwrap();
        assert_eq!(p.moves.len(), 2);
        assert_eq!(p.writes["views/v.md"].text, "# V\n\n[[x]] ![[concepts/a/h/p.png]]\n");
    }
}
