//! Concept rules (m0 §6): frontmatter fields by edition, fields a waiting update removes,
//! unique ids and slugs, and prerequisite cycles.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use super::{Ctx, update_code};
use crate::config::{Editions, UpdateId};
use crate::envelope::{Applicability, Diagnostic, Fix};
use crate::id::Ulid;
use crate::parse::{FmChange, FmValue, LinkForm, Note};
use crate::path::VaultPath;

/// A concept frontmatter field (m0 §6.1) and the editions it belongs to.
pub(super) struct Field {
    name: &'static str,
    pub(super) editions: Editions,
}

/// The concept fields, current and removed. A field a vault update removes keeps its
/// row with `Editions::until` that update: before it, the field is valid and reported
/// as outdated (`update.<id>`, with a fix); after it, it is an unknown field.
pub(super) const CONCEPT_FIELDS: &[Field] = &[
    Field { name: "id", editions: Editions::ALWAYS },
    Field { name: "prereqs", editions: Editions::ALWAYS },
    Field { name: "digest", editions: Editions::until(UpdateId::new(1)) },
];

/// The concepts whose frontmatter parsed (malformed frontmatter is reported as
/// `fm.*`; its fields cannot be checked). No frontmatter counts as an empty mapping.
fn concepts_with_fields<'a>(ctx: &'a Ctx) -> impl Iterator<Item = &'a Note> {
    ctx.index.concepts().filter(|n| !n.frontmatter_broken())
}

/// Field rules of a concept's frontmatter (m0 §6.1), by the vault's edition.
pub(super) fn concept_fields(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in concepts_with_fields(ctx) {
        for e in note.frontmatter.iter().flat_map(|f| f.entries()) {
            if !CONCEPT_FIELDS.iter().any(|f| f.name == e.key && f.editions.contains(ctx.mark)) {
                let msg = format!("field `{}` is not allowed in a concept", e.key);
                out.push(error(note, "fm.unknown-field", msg).at(&note.text, e.key_span.clone()));
            }
        }
        out.extend(id_field(note));
        out.extend(prereqs_field(note));
    }
}

fn error(note: &Note, code: &str, msg: String) -> Diagnostic {
    Diagnostic::error(code, msg).at_path(note.path.as_str())
}

/// `fm.field-type` for field `key` of `note`, at `span`.
fn wrong_type(note: &Note, key: &str, want: &str, span: Range<usize>) -> Diagnostic {
    error(note, "fm.field-type", format!("`{key}` must be {want}")).at(&note.text, span)
}

/// A fix making `change` to `note`'s frontmatter (m0 §6.1).
fn frontmatter_fix(note: &Note, applicability: Applicability, message: &str, change: FmChange) -> Fix {
    Fix::new(applicability, message).frontmatter(note.path.as_str(), change)
}

/// `id`: present, a string, a ULID. A missing one gets a fix adding a new id, first.
fn id_field(note: &Note) -> Option<Diagnostic> {
    match note.fm("id") {
        Some(FmValue::Str { value, span }) => {
            let e = value.parse::<Ulid>().err()?;
            Some(error(note, "fm.id-format", format!("`id` {e}")).at(&note.text, span.clone()))
        }
        Some(v) => Some(wrong_type(note, "id", "a string", v.span())),
        None => {
            // Only concepts whose frontmatter is a mapping, or absent, get here.
            let message = if note.frontmatter.is_some() { "add a new id" } else { "add frontmatter with a new id" };
            let fix = frontmatter_fix(note, Applicability::Safe, message, FmChange::SetId(Ulid::generate()));
            Some(error(note, "fm.missing-field", "concept has no `id`".into()).with_fix(fix))
        }
    }
}

/// `prereqs`: a list of strings `[[slug]]`. An unquoted `[[slug]]` gets a fix quoting
/// every such item of the note.
fn prereqs_field(note: &Note) -> Vec<Diagnostic> {
    let wrong = |span| wrong_type(note, "prereqs", "a list of `\"[[slug]]\"`", span).with_hint("write \"[[slug]]\"");
    let quote = || frontmatter_fix(note, Applicability::Safe, "quote it", FmChange::QuotePrereqs);
    match note.fm("prereqs") {
        None => vec![],
        Some(FmValue::List { items, .. }) => items
            .iter()
            .filter(|i| !note.links.iter().any(|l| l.form == LinkForm::Prereq && l.span == i.span()))
            .map(|i| match unquoted_prereq(i) {
                true => wrong(i.span()).with_fix(quote()),
                false => wrong(i.span()),
            })
            .collect(),
        Some(v) => vec![wrong(v.span())],
    }
}

/// Whether a `prereqs` item is an unquoted `[[slug]]`, which YAML reads as a list
/// holding a list holding the slug.
fn unquoted_prereq(item: &FmValue) -> bool {
    let FmValue::List { items, .. } = item else { return false };
    let [FmValue::List { items, .. }] = items.as_slice() else { return false };
    matches!(items.as_slice(), [FmValue::Str { value, .. }] if value.parse::<crate::id::Slug>().is_ok())
}

/// Concept fields a pending vault update removes (a row with `Editions::until` an
/// update the vault has not handled): valid in its edition, reported as outdated, with
/// the unsafe fix that deletes them (a field may hold text worth keeping).
pub(super) fn outdated_fields(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in concepts_with_fields(ctx) {
        for e in note.frontmatter.iter().flat_map(|f| f.entries()) {
            let Some(field) = CONCEPT_FIELDS.iter().find(|f| f.name == e.key && f.editions.contains(ctx.mark)) else {
                continue;
            };
            let Some(update) = field.editions.until else { continue };
            let path = note.path.as_str();
            let change = FmChange::RemoveField(e.key.clone());
            let fix = frontmatter_fix(note, Applicability::Unsafe, &format!("delete `{}`", e.key), change);
            let code = update_code(update);
            out.push(
                Diagnostic::warning(&code, format!("vault update {update} removes the concept field `{}`", e.key))
                    .at_path(path)
                    .at(&note.text, e.key_span.clone())
                    .with_hint(format!(
                        "offer {update} with `loci updates` if it is waiting; it applies with `loci lint --fix --only {code}`"
                    ))
                    .with_fix(fix),
            );
        }
    }
}

/// Concept ids and slugs unique in the vault.
pub(super) fn duplicates(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    let index = ctx.index;
    let mut ids: BTreeMap<Ulid, Vec<&Note>> = BTreeMap::new();
    for n in index.concepts() {
        if let Some(id) = n.id() {
            ids.entry(id).or_default().push(n);
        }
    }

    for (id, notes) in ids.iter().filter(|(_, ns)| ns.len() > 1) {
        // Unsafe: plugin data (a review log) may refer to the id of either note.
        let mut fix = Fix::new(Applicability::Unsafe, "give the other notes new ids");
        for n in &notes[1..] {
            fix = fix.frontmatter(n.path.as_str(), FmChange::SetId(Ulid::generate()));
        }
        let others: Vec<&str> = notes[1..].iter().map(|n| n.path.as_str()).collect();
        out.push(
            Diagnostic::error("fm.id-duplicate", format!("id `{id}` is used by {}", others.join(", ")))
                .at_path(notes[0].path.as_str())
                .with_fix(fix),
        );
    }

    for (slug, paths) in &index.slugs {
        if paths.len() > 1 {
            out.push(
                Diagnostic::error("slug.duplicate", format!("slug `{slug}` is used by {}", paths[1..].join(", ")))
                    .at_path(paths[0].as_str()),
            );
        }
    }
}

pub(super) fn prereq_cycles(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    let index = ctx.index;

    // Concept path -> prerequisite paths.
    let graph: BTreeMap<VaultPath, Vec<VaultPath>> =
        index.concepts().map(|n| (n.path.clone(), index.prereqs(n).0)).collect();

    // Iterative DFS; report each cycle once, from its member with the smallest slug.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Visit {
        Unseen,
        /// On the current DFS path: an edge back to it closes a cycle.
        OnPath,
        Done,
    }

    let mut state: BTreeMap<&VaultPath, Visit> = BTreeMap::new();
    let visit = |state: &BTreeMap<&VaultPath, Visit>, p: &VaultPath| state.get(p).copied().unwrap_or(Visit::Unseen);
    let mut reported: BTreeSet<Vec<VaultPath>> = BTreeSet::new();
    for start in graph.keys() {
        if visit(&state, start) != Visit::Unseen {
            continue;
        }
        let mut stack: Vec<(&VaultPath, usize)> = vec![(start, 0)];
        let mut path: Vec<&VaultPath> = vec![start];
        state.insert(start, Visit::OnPath);
        while let Some((node, i)) = stack.last_mut() {
            let edges = &graph[*node];
            if *i < edges.len() {
                let next = &edges[*i];
                *i += 1;
                match visit(&state, next) {
                    Visit::Unseen if graph.contains_key(next) => {
                        state.insert(next, Visit::OnPath);
                        stack.push((next, 0));
                        path.push(next);
                    }
                    Visit::OnPath => {
                        let pos = path.iter().position(|p| *p == next).expect("an `OnPath` node is on the path");
                        let mut cycle: Vec<VaultPath> = path[pos..].iter().map(|p| (*p).clone()).collect();
                        let min = cycle
                            .iter()
                            .enumerate()
                            .min_by_key(|(_, p)| (p.stem().to_lowercase(), p.as_str()))
                            .map(|(i, _)| i)
                            .expect("a cycle has members");
                        cycle.rotate_left(min);
                        if reported.insert(cycle.clone()) {
                            let mut shown: Vec<&str> = cycle.iter().map(VaultPath::stem).collect();
                            shown.push(cycle[0].stem());
                            out.push(
                                Diagnostic::error("prereqs.cycle", format!("prereq cycle: {}", shown.join(" → ")))
                                    .at_path(cycle[0].as_str()),
                            );
                        }
                    }
                    Visit::Unseen | Visit::Done => {}
                }
            } else {
                state.insert(node, Visit::Done);
                stack.pop();
                path.pop();
            }
        }
    }
}
