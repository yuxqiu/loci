//! Core lint rules (m0 §7).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::is_nfc;

use crate::config::Budgets;
use crate::digest;
use crate::envelope::{Diagnostic, sort_diagnostics};
use crate::git;
use crate::index::{FileIssue, Index, LinkError};
use crate::parse::{FmError, FmValue, LinkForm, Note, TopBlock, normalize_target, prereq_slug, verbatim};
use crate::sources::{SourceMeta, hex_sha256};
use crate::vault::{self, Kind};

static PAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^p[1-9][0-9]*$").unwrap());

const CONCEPT_FIELDS: [&str; 5] = ["id", "aliases", "prereqs", "sources", "digest"];

pub struct Lint<'a> {
    pub index: &'a Index,
    pub budgets: &'a Budgets,
    /// The vault root, for checks that read git or file bytes. `None` skips them.
    pub root: Option<&'a Path>,
}

impl Lint<'_> {
    pub fn run(&self) -> Vec<Diagnostic> {
        let mut out = vec![];
        self.files(&mut out);
        for note in self.index.notes.values() {
            self.note(note, &mut out);
        }
        self.vault_rules(&mut out);
        self.sources(&mut out);
        if let Some(root) = self.root {
            material_dropped(root, &mut out);
        }
        sort_diagnostics(&mut out);
        out
    }

    fn files(&self, out: &mut Vec<Diagnostic>) {
        for path in &self.index.files {
            if !is_nfc(path) {
                out.push(Diagnostic::error("path.not-nfc", "file name is not in Unicode NFC").at_path(path));
            }
            let kind = vault::kind_of(path);
            if kind == Some(Kind::Unknown) {
                out.push(Diagnostic::error("kind.unknown", "file is outside every known folder").at_path(path));
            }
            if kind == Some(Kind::Inbox) {
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
        if note.kind == Kind::Journal && !vault::journal_path_ok(path) {
            out.push(d(
                "journal.path",
                "journal entry is not inside a `journal/<stream>/` folder whose name is a slug".into(),
            ));
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
        let d = |code: &str, msg: String| Diagnostic::error(code, msg).at_path(path);
        let slug = vault::stem(path);
        if !vault::is_slug(slug) {
            out.push(d("slug.format", format!("slug `{slug}` does not match the slug pattern")));
        }
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
        let get = |key: &str| note.frontmatter.as_ref().and_then(|f| f.get(key)).map(|e| &e.value);
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
        match get("aliases") {
            None => {}
            Some(FmValue::List { items, .. }) => {
                for i in items {
                    if !matches!(i, FmValue::Str { value, .. } if !value.trim().is_empty()) {
                        out.push(ty("aliases", "a list of non-empty strings", i.span()));
                    }
                }
            }
            Some(v) => out.push(ty("aliases", "a list of non-empty strings", v.span())),
        }
        let prereq = |span| ty("prereqs", "a list of `\"[[slug]]\"`", span).with_hint("write \"[[slug]]\"");
        match get("prereqs") {
            None => {}
            Some(FmValue::List { items, .. }) => {
                for i in items {
                    let ok = matches!(i, FmValue::Str { value, span, quoted: true }
                        if prereq_slug(value).is_some() && verbatim(text, span, value, true));
                    if !ok {
                        out.push(prereq(i.span()));
                    }
                }
            }
            Some(v) => out.push(prereq(v.span())),
        }
        let sources_ty = "a list of strings without escapes";
        match get("sources") {
            None => {}
            Some(FmValue::List { items, .. }) => {
                for i in items.iter().filter(
                    |i| !matches!(i, FmValue::Str { value, span, quoted } if verbatim(text, span, value, *quoted)),
                ) {
                    out.push(ty("sources", sources_ty, i.span()));
                }
            }
            Some(v) => out.push(ty("sources", sources_ty, v.span())),
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
            if link.form == LinkForm::Source {
                if !self.source_ok(link) {
                    out.push(at("sources.unresolved", format!("source `{}` does not resolve", link.raw)));
                }
                continue;
            }
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

    /// `sources:` entries (§5.5).
    fn source_ok(&self, link: &crate::parse::Link) -> bool {
        let (target, frag) = (link.target.as_str(), link.fragment.as_deref());
        if let Some(folder) = target.strip_prefix("sources/") {
            let parts: Vec<&str> = folder.split('/').collect();
            if parts.len() != 2 {
                return false;
            }
            let prefix = format!("{target}/");
            let files: Vec<&String> =
                self.index.files.iter().filter(|f| f.starts_with(&prefix) && !f.ends_with("/source.yaml")).collect();
            if files.is_empty() && !self.index.files.iter().any(|f| f.starts_with(&prefix)) {
                return false;
            }
            return match frag {
                None => true,
                Some(f) if PAGE.is_match(f) => files.len() == 1 && files[0].ends_with(".pdf"),
                Some(f) => {
                    files.len() == 1 && files[0].ends_with(".md") && self.index.check_fragment(files[0], f).is_ok()
                }
            };
        }
        if target.starts_with("journal/") {
            let Some((t, _)) = normalize_target(target) else { return false };
            let note = format!("{t}.md");
            if !self.index.notes.contains_key(&note) {
                return false;
            }
            return frag.is_none_or(|f| !f.starts_with('^') && self.index.check_fragment(&note, f).is_ok());
        }
        false
    }

    fn vault_rules(&self, out: &mut Vec<Diagnostic>) {
        let index = self.index;
        // Duplicate ids.
        let mut ids: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for n in index.concepts() {
            if let Some(FmValue::Str { value, .. }) = n.frontmatter.as_ref().and_then(|f| f.get("id")).map(|e| &e.value)
            {
                ids.entry(value.clone()).or_default().push(&n.path);
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
        for (alias, paths) in &index.aliases {
            let mut others: Vec<&str> = paths.iter().map(String::as_str).collect();
            if let Some(slug_paths) = index.slugs.get(alias) {
                others.extend(slug_paths.iter().map(String::as_str));
            }
            others.sort();
            others.dedup();
            if paths.len() > 1 || index.slugs.contains_key(alias) {
                out.push(
                    Diagnostic::error(
                        "alias.duplicate",
                        format!("alias `{alias}` equals a slug or alias of {}", others[1..].join(", ")),
                    )
                    .at_path(others[0]),
                );
            }
        }
        // Areas.
        for a in index.areas() {
            let p = format!("concepts/{a}/AREA.md");
            if !index.notes.contains_key(&p) {
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

    /// Source folders (§6.4).
    fn sources(&self, out: &mut Vec<Diagnostic>) {
        let mut folders: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for f in self.index.files.iter().filter(|f| f.starts_with("sources/")) {
            let parts: Vec<&str> = f.split('/').collect();
            if parts.len() < 4 || !vault::is_slug(parts[1]) || !vault::is_slug(parts[2]) {
                out.push(
                    Diagnostic::error(
                        "sources.layout",
                        format!("`{f}` is not inside a `sources/<type>/<slug>/` folder"),
                    )
                    .at_path(f),
                );
                continue;
            }
            folders.entry(parts[..3].join("/")).or_default().push(parts[3..].join("/"));
        }
        for (folder, files) in folders {
            let meta = format!("{folder}/source.yaml");
            let at = |code: &str, msg: String| Diagnostic::error(code, msg).at_path(meta.as_str());
            let others: Vec<&String> = files.iter().filter(|f| f.as_str() != "source.yaml").collect();
            if others.len() != 1 {
                out.push(at(
                    "sources.uncovered",
                    "source folder must hold exactly one file besides `source.yaml`".into(),
                ));
            }
            if !files.iter().any(|f| f == "source.yaml") {
                out.push(at("sources.no-provenance", "source folder has no `source.yaml`".into()));
                continue;
            }
            let Some(root) = self.root else { continue };
            let unreadable = |path: &str, e: std::io::Error| {
                Diagnostic::error("file.unreadable", format!("file cannot be read: {e}")).at_path(path)
            };
            let meta_text = match std::fs::read_to_string(root.join(&meta)) {
                Ok(t) => t,
                Err(e) => {
                    out.push(unreadable(&meta, e));
                    continue;
                }
            };
            let meta_parsed = match SourceMeta::parse(&meta_text) {
                Ok(m) => m,
                Err(reason) => {
                    out.push(at("sources.bad-meta", format!("`source.yaml` is malformed: {reason}")));
                    continue;
                }
            };
            let sha = meta_parsed.sha256.as_str();
            if others.len() == 1 {
                let file = format!("{folder}/{}", others[0]);
                match std::fs::read(root.join(&file)) {
                    Ok(bytes) if hex_sha256(&bytes) != sha => out.push(
                        Diagnostic::error(
                            "sources.hash-mismatch",
                            format!("`{file}` does not match its recorded SHA-256"),
                        )
                        .at_path(file.as_str()),
                    ),
                    Ok(_) => {}
                    Err(e) => out.push(unreadable(&file, e)),
                }
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

/// `material-dropped` (§7.3): compares the working tree with HEAD. A git failure is
/// warning `git.failed`, and the rule is not checked.
pub fn material_dropped(root: &Path, out: &mut Vec<Diagnostic>) {
    if !git::has_head(root) {
        return;
    }
    if let Err(e) = check_material(root, out) {
        out.push(Diagnostic::warning("git.failed", format!("git failed, `material-dropped` was not checked: {e}")));
    }
}

fn check_material(root: &Path, out: &mut Vec<Diagnostic>) -> Result<(), String> {
    const CATEGORIES: [&str; 3] = ["math spans", "images", "fenced code blocks"];
    let counts = |text: &[u8], path: &str| -> [usize; 3] {
        // A file that is not UTF-8 holds no parsed material (lint reports it).
        let Ok(text) = std::str::from_utf8(text) else { return [0; 3] };
        let c = crate::parse::parse(path, vault::kind_of(path).unwrap_or(Kind::Unknown), text.to_string()).counts;
        [c.math, c.images, c.code]
    };
    let head = |p: &str| git::show(root, "HEAD", p).map(|b| counts(&b, p));
    let changes = git::status(root)?;
    let mut deleted = [0usize; 3];
    let mut added = [0usize; 3];
    // Per category, the first deleted path that held any (§7.1: vault-level rules
    // report on the first involved path).
    let mut first: [Option<&str>; 3] = [None; 3];
    let mut deleted_paths: Vec<&String> = changes.deleted.iter().filter(|p| p.ends_with(".md")).collect();
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
    for (p, in_head) in changed.filter(|(p, _)| p.ends_with(".md")) {
        let bytes = std::fs::read(root.join(p)).map_err(|e| format!("reading `{p}`: {e}"))?;
        let now = counts(&bytes, p);
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
        Lint { index: &index, budgets: &budgets, root: None }
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
            assert!(index.notes["concepts/a/y.md"].links.is_empty(), "{bad} is not a link");
        }
    }

    #[test]
    fn escaped_frontmatter_links_are_rejected_not_linked() {
        let note = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\nprereqs: [\"[[caf\\u00e9]]\"]\nsources: [\"views/\\x61\"]\ndigest: d\n---\n# X\n";
        let files = [("concepts/a/x.md", note), ("concepts/a/AREA.md", "# A\n\nArea.\n"), ("concepts/a/café.md", "")];
        let d = lint(&files);
        let x = |code: &str| d.iter().filter(|(p, c)| p == "concepts/a/x.md" && c == code).count();
        assert_eq!(x("fm.field-type"), 2, "{d:?}");
        let index = crate::index::from_texts(&files);
        assert!(index.notes["concepts/a/x.md"].links.is_empty(), "no link from an escaped scalar");
    }

    const OK: &str =
        "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\nsources: [\"journal/r/2025/03/25-03-14\"]\ndigest: A thing.\n---\n# A\n";

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
