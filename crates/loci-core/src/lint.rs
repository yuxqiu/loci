//! Core lint rules (m0 §7).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use unicode_normalization::is_nfc;

use crate::config::Budgets;
use crate::digest;
use crate::envelope::{Diagnostic, sort_diagnostics};
use crate::git;
use crate::index::{FileIssue, Index, LinkError};
use crate::parse::{FmError, FmValue, Note, TopBlock, prereq_item};
use crate::vault::{self, Kind};

const CONCEPT_FIELDS: [&str; 3] = ["id", "prereqs", "digest"];

/// The rules over the index (§7.3). The rules that read file bytes or git are
/// [`disk_rules`].
pub struct Lint<'a> {
    pub index: &'a Index,
    pub budgets: &'a Budgets,
}

impl Lint<'_> {
    /// Sorted diagnostics.
    pub fn run(&self) -> Vec<Diagnostic> {
        let mut out = vec![];
        self.files(&mut out);
        for note in self.index.markdown.values() {
            self.note(note, &mut out);
        }
        self.vault_rules(&mut out);
        sort_diagnostics(&mut out);
        out
    }

    fn files(&self, out: &mut Vec<Diagnostic>) {
        for path in &self.index.files {
            // Path rules (§3, §6.2, §6.4, §6.6), shared with `new` and `mv`.
            if let Some((code, message)) = vault::path_problem(path) {
                out.push(Diagnostic::error(code, message).at_path(path.as_str()));
            }
            if !is_nfc(path) {
                out.push(Diagnostic::error("path.not-nfc", "file name is not in Unicode NFC").at_path(path));
            }
            if vault::kind_of(path) == Some(Kind::Inbox) {
                let parts: Vec<&str> = path.split('/').collect();
                let root_file = parts.len() == 2 || (parts.len() == 3 && parts[1] == "journal");
                if root_file {
                    out.push(
                        Diagnostic::warning(
                            "inbox.root-file",
                            "file directly in `inbox/` or `inbox/journal/`; move it into `drafts/`, `sources/` or a journal stream",
                        )
                        .at_path(path),
                    );
                }
            }
        }
        for (path, issue) in &self.index.issues {
            match issue {
                FileIssue::NotUtf8 => {
                    out.push(Diagnostic::error("file.encoding", "file is not valid UTF-8").at_path(path))
                }
                FileIssue::Unreadable(reason) => out
                    .push(Diagnostic::error("file.unreadable", format!("file cannot be read: {reason}")).at_path(path)),
            }
        }
    }

    fn note(&self, note: &Note, out: &mut Vec<Diagnostic>) {
        if !note.kind.is_managed() {
            return;
        }
        let path = note.path.as_str();
        let text = note.text.as_str();
        let d = |code: &str, msg: String| Diagnostic::error(code, msg).at_path(path);
        if note.bom {
            out.push(d("file.bom", "file starts with a byte-order mark".into()));
        }
        match &note.fm_error {
            Some(FmError::Unterminated) => {
                out.push(d("fm.unterminated", "frontmatter opened with `---` is not a closed block".into()))
            }
            Some(FmError::Yaml(reason)) => {
                let r = note.frontmatter.as_ref().map_or(0..0, |f| f.range.clone());
                out.push(d("fm.yaml", format!("frontmatter is not a YAML mapping: {reason}")).at(text, r));
            }
            None => {}
        }
        if note.kind != Kind::Concept {
            if let Some(fm) = &note.frontmatter {
                out.push(
                    d("fm.not-allowed", format!("{} files have no frontmatter", note.kind.name()))
                        .at(text, fm.range.clone()),
                );
            }
        } else {
            self.concept(note, out);
        }
        h1_rules(note, out);
        if note.kind == Kind::Area {
            area_rules(note, self.budgets, out);
        }
        self.links(note, out);
        // Block ids unique within the note.
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for b in &note.block_ids {
            if !seen.insert(b.id.as_str()) {
                out.push(
                    d("block.id-duplicate", format!("block id `^{}` appears more than once in this note", b.id))
                        .at(text, b.range.clone()),
                );
            }
        }
    }

    fn concept(&self, note: &Note, out: &mut Vec<Diagnostic>) {
        let path = note.path.as_str();
        let text = note.text.as_str();
        let body_chars = text[note.body_start()..].chars().count();
        if body_chars > self.budgets.note {
            out.push(
                Diagnostic::warning(
                    "size.note",
                    format!("note body is {body_chars} characters (budget {})", self.budgets.note),
                )
                .at_path(path),
            );
        }
        // Malformed frontmatter is reported as `fm.*`; its fields cannot be checked.
        if note.fm_error.is_none() {
            self.concept_fields(note, out);
        }
    }

    /// Field rules of a concept's frontmatter (m0 §6.1). No frontmatter counts as an
    /// empty mapping.
    fn concept_fields(&self, note: &Note, out: &mut Vec<Diagnostic>) {
        let path = note.path.as_str();
        let text = note.text.as_str();
        let d = |code: &str, msg: String| Diagnostic::error(code, msg).at_path(path);
        let ty = |key: &str, t: &str, span: std::ops::Range<usize>| {
            d("fm.field-type", format!("`{key}` must be {t}")).at(text, span)
        };
        let get = |key: &str| note.fm(key);
        for e in note.frontmatter.iter().flat_map(|f| &f.entries) {
            if !CONCEPT_FIELDS.contains(&e.key.as_str()) {
                out.push(
                    d("fm.unknown-field", format!("field `{}` is not allowed in a concept", e.key))
                        .at(text, e.key_span.clone()),
                );
            }
        }
        match get("id") {
            None => out.push(d("fm.missing-field", "concept has no `id`".into())),
            Some(FmValue::Str { value, span, .. }) => {
                if !vault::is_ulid(value) {
                    out.push(d("fm.id-format", format!("`id` `{value}` is not a ULID")).at(text, span.clone()));
                }
            }
            Some(v) => out.push(ty("id", "a string", v.span())),
        }
        // `prereqs`: a list of `"[[slug]]"`, written verbatim.
        let wrong = |span| ty("prereqs", "a list of `\"[[slug]]\"`", span).with_hint("write \"[[slug]]\"");
        match get("prereqs") {
            None => {}
            Some(FmValue::List { items, .. }) => {
                out.extend(items.iter().filter(|i| prereq_item(text, i).is_none()).map(|i| wrong(i.span())))
            }
            Some(v) => out.push(wrong(v.span())),
        }
        match get("digest") {
            None => out.push(d("fm.missing-field", "concept has no `digest`".into())),
            Some(FmValue::Str { value, span, .. }) => {
                for (code, msg) in digest::digest_errors(value) {
                    out.push(d(code, msg.into()).at(text, span.clone()));
                }
                let n = value.chars().count();
                if !value.trim().is_empty() && n > self.budgets.digest {
                    out.push(
                        Diagnostic::warning(
                            "digest.too-long",
                            format!("`digest` is {n} characters (budget {})", self.budgets.digest),
                        )
                        .at_path(path)
                        .at(text, span.clone()),
                    );
                }
            }
            Some(v) => out.push(ty("digest", "a string", v.span())),
        }
    }

    fn links(&self, note: &Note, out: &mut Vec<Diagnostic>) {
        let path = note.path.as_str();
        let text = note.text.as_str();
        for m in &note.md_links {
            // Notes are linked without `.md`; other files keep their extension.
            let url = m.url.strip_prefix("./").unwrap_or(&m.url);
            let wiki =
                if m.image { format!("![[{url}]]") } else { format!("[[{}]]", url.strip_suffix(".md").unwrap_or(url)) };
            out.push(
                Diagnostic::error("link.markdown", format!("a vault target must use a wikilink: `{wiki}`"))
                    .at_path(path)
                    .at(text, m.range.clone())
                    .with_hint(format!("write {wiki}")),
            );
        }
        for link in &note.links {
            let at = |code: &str, msg: String| {
                Diagnostic::error(code, msg).at_path(path).at(text, link.target_range.clone())
            };
            match self.index.resolve(note, link) {
                Ok(_) => {}
                Err(LinkError::Dangling { hint }) => {
                    let mut diag = at("link.dangling", format!("`{}` does not resolve", link.raw));
                    diag.hint = hint;
                    out.push(diag);
                }
                Err(LinkError::PathForm) => {
                    out.push(at("link.path-form", "`..` or a leading `/` is not allowed in a link".into()))
                }
                Err(LinkError::Heading) => out.push(at(
                    "link.dangling-heading",
                    format!(
                        "`{}` has no heading `{}`",
                        target_name(note, link),
                        link.fragment.as_deref().unwrap_or("")
                    ),
                )),
                Err(LinkError::Block) => out.push(at(
                    "link.dangling-block",
                    format!("`{}` has no block `{}`", target_name(note, link), link.fragment.as_deref().unwrap_or("")),
                )),
            }
        }
    }

    fn vault_rules(&self, out: &mut Vec<Diagnostic>) {
        let index = self.index;
        // Duplicate ids.
        let mut ids: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for n in index.concepts() {
            if let Some(id) = n.fm_str("id") {
                ids.entry(id.to_string()).or_default().push(&n.path);
            }
        }
        for (id, paths) in &ids {
            if paths.len() > 1 {
                out.push(
                    Diagnostic::error("fm.id-duplicate", format!("id `{id}` is used by {}", paths[1..].join(", ")))
                        .at_path(paths[0]),
                );
            }
        }
        for (slug, paths) in &index.slugs {
            if paths.len() > 1 {
                out.push(
                    Diagnostic::error("slug.duplicate", format!("slug `{slug}` is used by {}", paths[1..].join(", ")))
                        .at_path(paths[0].as_str()),
                );
            }
        }
        // Areas.
        for a in index.areas() {
            let p = format!("concepts/{a}/AREA.md");
            if !index.markdown.contains_key(&p) {
                out.push(Diagnostic::error("area.missing", format!("area `{a}` has no AREA.md")).at_path(p));
            }
        }
        // Prereq cycles.
        prereq_cycles(index, out);
        // Index budgets.
        let all = digest::areas(index);
        let root = digest::render_root(&all);
        let n = root.chars().count();
        if n > self.budgets.root_index {
            out.push(Diagnostic::warning(
                "index.over-budget",
                format!("the root digest output is {n} characters (budget {})", self.budgets.root_index),
            ));
        }
        for a in &all {
            let n = digest::render_area(a).chars().count();
            if n > self.budgets.area_index {
                out.push(
                    Diagnostic::warning(
                        "index.over-budget",
                        format!("the {} digest output is {n} characters (budget {})", a.name, self.budgets.area_index),
                    )
                    .at_path(format!("concepts/{}/AREA.md", a.name)),
                );
            }
        }
    }
}

fn target_name(from: &Note, link: &crate::parse::Link) -> String {
    if link.target.is_empty() { vault::stem(&from.path).to_string() } else { link.target.clone() }
}

fn h1_rules(note: &Note, out: &mut Vec<Diagnostic>) {
    let path = note.path.as_str();
    let h1s: Vec<_> = note.headings.iter().filter(|h| h.level == 1).collect();
    if h1s.is_empty() {
        out.push(Diagnostic::error("h1.missing", "no H1").at_path(path));
        return;
    }
    if h1s.len() > 1 {
        out.push(
            Diagnostic::error("h1.multiple", "more than one H1").at_path(path).at(&note.text, h1s[1].range.clone()),
        );
    }
    if !matches!(note.top_blocks.first(), Some((TopBlock::Heading(1), _))) {
        out.push(
            Diagnostic::error("h1.not-first", "the H1 is not the first block after the frontmatter")
                .at_path(path)
                .at(&note.text, h1s[0].range.clone()),
        );
    }
}

fn area_rules(note: &Note, budgets: &Budgets, out: &mut Vec<Diagnostic>) {
    let path = note.path.as_str();
    match digest::area_parts(note) {
        None => out.push(Diagnostic::error("area.format", "AREA.md must be an H1 and one paragraph").at_path(path)),
        Some((_, para)) => {
            let n = para.chars().count();
            if n > budgets.area_paragraph {
                out.push(
                    Diagnostic::warning(
                        "area.too-long",
                        format!("area paragraph is {n} characters (budget {})", budgets.area_paragraph),
                    )
                    .at_path(path),
                );
            }
        }
    }
}

fn prereq_cycles(index: &Index, out: &mut Vec<Diagnostic>) {
    // Concept path -> prerequisite paths.
    let graph: BTreeMap<String, Vec<String>> = index.concepts().map(|n| (n.path.clone(), index.prereqs(n).0)).collect();
    // Iterative DFS with colours; report each cycle once, from its member with the
    // smallest slug.
    let mut state: BTreeMap<&str, u8> = BTreeMap::new();
    let mut reported: BTreeSet<Vec<String>> = BTreeSet::new();
    for start in graph.keys() {
        if state.get(start.as_str()).copied().unwrap_or(0) != 0 {
            continue;
        }
        let mut stack: Vec<(&str, usize)> = vec![(start.as_str(), 0)];
        let mut path: Vec<&str> = vec![start.as_str()];
        state.insert(start.as_str(), 1);
        while let Some((node, i)) = stack.last_mut() {
            let edges = &graph[*node];
            if *i < edges.len() {
                let next = edges[*i].as_str();
                *i += 1;
                match state.get(next).copied().unwrap_or(0) {
                    0 if graph.contains_key(next) => {
                        state.insert(next, 1);
                        stack.push((next, 0));
                        path.push(next);
                    }
                    1 => {
                        let pos = path.iter().position(|p| *p == next).expect("a grey node is on the path");
                        let mut cycle: Vec<String> = path[pos..].iter().map(|s| s.to_string()).collect();
                        let min = cycle
                            .iter()
                            .enumerate()
                            .min_by_key(|(_, p)| (vault::stem(p).to_lowercase(), p.as_str()))
                            .map(|(i, _)| i)
                            .expect("a cycle has members");
                        cycle.rotate_left(min);
                        if reported.insert(cycle.clone()) {
                            let mut shown: Vec<&str> = cycle.iter().map(|p| vault::stem(p)).collect();
                            shown.push(vault::stem(&cycle[0]));
                            out.push(
                                Diagnostic::error("prereqs.cycle", format!("prereq cycle: {}", shown.join(" → ")))
                                    .at_path(cycle[0].as_str()),
                            );
                        }
                    }
                    _ => {}
                }
            } else {
                state.insert(node, 2);
                stack.pop();
                path.pop();
            }
        }
    }
}

/// The rules that compare the working tree with HEAD (§7.3): `sources.changed` and
/// `material-dropped`. Without a HEAD there is nothing to compare; a git failure is
/// warning `git.failed`, and neither rule is checked. Unsorted; the caller merges them
/// with [`Lint::run`].
pub fn disk_rules(index: &Index, root: &Path) -> Vec<Diagnostic> {
    let mut out = vec![];
    let changes = match git::has_head(root).and_then(|head| head.then(|| git::status(root)).transpose()) {
        Ok(Some(changes)) => changes,
        Ok(None) => return out,
        Err(e) => {
            let msg = format!("git failed, `sources.changed` and `material-dropped` were not checked: {e}");
            out.push(Diagnostic::warning("git.failed", msg));
            return out;
        }
    };
    sources_changed(&changes, &mut out);
    if let Err(e) = check_material(index, root, &changes, &mut out) {
        out.push(Diagnostic::warning("git.failed", format!("git failed, `material-dropped` was not checked: {e}")));
    }
    out
}

/// `sources.changed` (§6.4): a source's content differs from HEAD at the same path.
fn sources_changed(changes: &git::Changes, out: &mut Vec<Diagnostic>) {
    for p in changes.modified.iter().filter(|p| vault::is_source_file(p)) {
        out.push(
            Diagnostic::error("sources.changed", format!("source `{p}` differs from its committed version"))
                .at_path(p.as_str())
                .with_hint("sources never change; add a new source instead"),
        );
    }
}

/// Only vault files count (m0 §3): a deleted path that core would read, and a current
/// file the index lists, whose counts the index already holds. Only reading `HEAD` can
/// fail.
fn check_material(index: &Index, root: &Path, changes: &git::Changes, out: &mut Vec<Diagnostic>) -> Result<(), String> {
    const CATEGORIES: [&str; 3] = ["math spans", "images", "fenced code blocks"];
    let of = |c: &crate::parse::Counts| [c.math, c.images, c.code];
    // A vault path that is not UTF-8 holds no parsed material (lint reports it).
    let head = |p: &str| {
        git::show(root, "HEAD", p).map(|b| match String::from_utf8(b) {
            Ok(text) => of(&crate::parse::parse(p, vault::kind_of(p).expect("a vault path"), text).counts),
            Err(_) => [0; 3],
        })
    };
    let mut deleted = [0usize; 3];
    let mut added = [0usize; 3];
    // Per category, the first deleted path that held any (§7.1: vault-level rules
    // report on the first involved path).
    let mut first: [Option<&str>; 3] = [None; 3];
    let mut deleted_paths: Vec<&String> =
        changes.deleted.iter().filter(|p| p.ends_with(".md") && vault::is_core_path(p)).collect();
    deleted_paths.sort();
    for p in deleted_paths {
        for (i, n) in head(p)?.into_iter().enumerate() {
            deleted[i] += n;
            if n > 0 && first[i].is_none() {
                first[i] = Some(p);
            }
        }
    }
    let new_files = changes.added.iter().chain(&changes.untracked).map(|p| (p, false));
    let changed = changes.modified.iter().map(|p| (p, true)).chain(new_files);
    let listed = |p: &str| index.files.binary_search_by(|f| f.as_str().cmp(p)).is_ok();
    for (p, in_head) in changed.filter(|(p, _)| p.ends_with(".md") && listed(p)) {
        // An unreadable or non-UTF-8 note is not in `markdown` (lint reports it): it holds none.
        let now = index.markdown.get(p).map_or([0; 3], |n| of(&n.counts));
        let before = if in_head { head(p)? } else { [0; 3] };
        for i in 0..3 {
            added[i] += now[i].saturating_sub(before[i]);
        }
    }
    for (i, name) in CATEGORIES.iter().enumerate() {
        if deleted[i] > added[i] {
            let path = first[i].expect("deleted[i] > 0 implies a deleted file held some");
            out.push(
                Diagnostic::warning(
                    "material-dropped",
                    format!("deleted files held {} {name} but only {} were added elsewhere", deleted[i], added[i]),
                )
                .at_path(path),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(files: &[(&str, &str)]) -> Vec<(String, String)> {
        let index = crate::index::from_texts(files);
        let budgets = Budgets::default();
        Lint { index: &index, budgets: &budgets }
            .run()
            .into_iter()
            .map(|d| (d.path.unwrap_or_default(), d.code))
            .collect()
    }

    #[test]
    fn prereqs_must_be_bare_slugs() {
        for bad in ["[[x#H]]", "[[]]", "[[x|y]]", "[[a/x]]", "[[two words]]"] {
            let note = format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\nprereqs: [\"{bad}\"]\ndigest: d\n---\n# X\n");
            let files = [("concepts/a/y.md", note.as_str()), ("concepts/a/x.md", "")];
            let d = lint(&files);
            assert!(d.contains(&("concepts/a/y.md".into(), "fm.field-type".into())), "{bad}: {d:?}");
            let index = crate::index::from_texts(&files);
            assert!(index.markdown["concepts/a/y.md"].links.is_empty(), "{bad} is not a link");
        }
    }

    #[test]
    fn escaped_frontmatter_links_are_rejected_not_linked() {
        let note = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\nprereqs: [\"[[caf\\u00e9]]\"]\ndigest: d\n---\n# X\n";
        let files = [("concepts/a/x.md", note), ("concepts/a/AREA.md", "# A\n\nArea.\n"), ("concepts/a/café.md", "")];
        let d = lint(&files);
        let x = |code: &str| d.iter().filter(|(p, c)| p == "concepts/a/x.md" && c == code).count();
        assert_eq!(x("fm.field-type"), 1, "{d:?}");
        let index = crate::index::from_texts(&files);
        assert!(index.markdown["concepts/a/x.md"].links.is_empty(), "no link from an escaped scalar");
    }

    const OK: &str =
        "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\ndigest: A thing.\n---\n# A\n\nSee [[journal/r/2025/03/25-03-14]].\n";

    #[test]
    fn clean_vault() {
        let r = lint(&[
            ("concepts/x/a.md", OK),
            ("concepts/x/AREA.md", "# X\n\nThe area.\n"),
            ("journal/r/2025/03/25-03-14.md", "# J\n\n[[a]]\n"),
        ]);
        assert_eq!(r, vec![]);
    }

    #[test]
    fn journal_paths() {
        assert!(vault::journal_path_ok("journal/research/2025/03/25-03-14.md"));
        assert!(vault::journal_path_ok("journal/life/2025-03-14 rent.md"), "names below the stream are free");
        assert!(vault::journal_path_ok("journal/life/a/b/notes.md"));
        assert!(!vault::journal_path_ok("journal/x.md"), "outside a stream");
        assert!(!vault::journal_path_ok("journal/My Stuff/2025/03/25-03-14.md"), "the stream is a slug");
        assert!(!vault::journal_path_ok("journal/Research/2025/03/25-03-14.md"), "the stream is a slug");
    }

    #[test]
    fn cycle_reported_once() {
        let c = |p: &str| format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\nprereqs: [\"[[{p}]]\"]\ndigest: d\n---\n# T\n");
        let r =
            lint(&[("concepts/x/AREA.md", "# X\n\nP.\n"), ("concepts/x/a.md", &c("b")), ("concepts/x/b.md", &c("a"))]);
        assert_eq!(r.iter().filter(|(_, c)| c == "prereqs.cycle").count(), 1);
    }

    #[test]
    fn tags_and_unquoted_prereqs() {
        let r = lint(&[
            ("concepts/x/AREA.md", "# X\n\nP.\n"),
            (
                "concepts/x/a.md",
                "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\ntags: [a]\nprereqs: [[b]]\ndigest: d\n---\n# T\n",
            ),
        ]);
        let codes: Vec<&str> = r.iter().map(|(_, c)| c.as_str()).collect();
        assert!(codes.contains(&"fm.unknown-field"));
        assert!(codes.contains(&"fm.field-type"));
    }
}
