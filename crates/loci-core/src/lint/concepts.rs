//! Concept rules (m0 §6): frontmatter fields by edition, fields a waiting update removes,
//! unique ids and slugs, and prerequisite cycles.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use super::{Code, Ctx};
use crate::config::{Editions, UpdateId};
use crate::envelope::{Applicability, Diagnostic, Fix};
use crate::id::Ulid;
use crate::parse::{ConceptField, FmChange, FmValue, Note, prereq_slug};
use crate::path::VaultPath;

/// A concept frontmatter field (m0 §6.1) and the editions it belongs to.
pub(super) struct Field {
    field: ConceptField,
    pub(super) editions: Editions,
}

/// The concept fields, current and removed. A field a vault update removes keeps its
/// row with `Editions::until` that update: before it, the field is valid and reported
/// as outdated (`update.<id>`, with a fix); after it, it is an unknown field.
pub(super) const CONCEPT_FIELDS: &[Field] = &[
    Field { field: ConceptField::Id, editions: Editions::ALWAYS },
    Field { field: ConceptField::Prereqs, editions: Editions::ALWAYS },
    Field { field: ConceptField::Digest, editions: Editions::until(UpdateId::new(1)) },
];

/// The row of the concept field with key `key` in edition `mark`, if it is one.
fn row(key: &str, mark: Option<UpdateId>) -> Option<&'static Field> {
    CONCEPT_FIELDS.iter().find(|f| f.field.key() == key && f.editions.contains(mark))
}

/// The concepts whose frontmatter parsed (malformed frontmatter is reported as
/// `fm.*`; its fields cannot be checked). No frontmatter counts as an empty mapping.
fn concepts_with_fields<'a>(ctx: &'a Ctx) -> impl Iterator<Item = &'a Note> {
    ctx.index.concepts().filter(|n| !n.has_broken_frontmatter())
}

/// Field rules of a concept's frontmatter (m0 §6.1), by the vault's edition.
pub(super) fn concept_fields(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in concepts_with_fields(ctx) {
        for e in note.frontmatter.iter().flat_map(|f| f.entries()) {
            if row(&e.key, ctx.mark).is_none() {
                let msg = format!("field `{}` is not allowed in a concept", *e.key);
                out.push(Diagnostic::error(Code::FmUnknownField, msg).in_note(note, e.key.span.clone()));
            }
        }
        out.extend(id_field(note));
        out.extend(prereqs_field(note));
    }
}

/// `fm.field-type` for `field` of `note`, at `span`.
fn wrong_type(note: &Note, field: ConceptField, want: &str, span: Range<usize>) -> Diagnostic {
    Diagnostic::error(Code::FmFieldType, format!("`{}` must be {want}", field.key())).in_note(note, span)
}

/// `id`: present, a string, a ULID. A missing one gets a fix adding a new id, first.
fn id_field(note: &Note) -> Option<Diagnostic> {
    match note.fm(ConceptField::Id.key()).map(|v| (&v.node, &v.span)) {
        Some((FmValue::Str(value), span)) => {
            let e = value.parse::<Ulid>().err()?;
            Some(Diagnostic::error(Code::FmIdFormat, format!("`id` {e}")).in_note(note, span.clone()))
        }
        Some((_, span)) => Some(wrong_type(note, ConceptField::Id, "a string", span.clone())),
        None => {
            // Only concepts whose frontmatter is a mapping, or absent, get here.
            let message = if note.frontmatter.is_some() { "add a new id" } else { "add frontmatter with a new id" };
            let fix = Fix::new(Applicability::Safe, message).frontmatter(&note.path, FmChange::SetId(Ulid::generate()));
            Some(Diagnostic::error(Code::FmMissingField, "concept has no `id`").at_path(&note.path).with_fix(fix))
        }
    }
}

/// `prereqs`: a list of strings `[[slug]]`. An unquoted `[[slug]]` gets a fix quoting
/// every such item of the note.
fn prereqs_field(note: &Note) -> Vec<Diagnostic> {
    let wrong = |span| {
        wrong_type(note, ConceptField::Prereqs, "a list of `\"[[slug]]\"`", span).with_hint("write \"[[slug]]\"")
    };
    let quote = || Fix::new(Applicability::Safe, "quote it").frontmatter(&note.path, FmChange::QuotePrereqs);
    match note.fm(ConceptField::Prereqs.key()).map(|v| (&v.node, &v.span)) {
        None => vec![],
        Some((FmValue::List(items), _)) => items
            .iter()
            // An item is a prereq link exactly when the parser read one from it.
            .filter(|i| !matches!(&i.node, FmValue::Str(item) if prereq_slug(item).is_some()))
            .map(|i| match i.is_unquoted_prereq() {
                true => wrong(i.span.clone()).with_fix(quote()),
                false => wrong(i.span.clone()),
            })
            .collect(),
        Some((_, span)) => vec![wrong(span.clone())],
    }
}

/// Concept fields a pending vault update removes (a row with `Editions::until` an
/// update the vault has not handled): valid in its edition, reported as outdated, with
/// the unsafe fix that deletes them (a field may hold text worth keeping).
pub(super) fn outdated_fields(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for note in concepts_with_fields(ctx) {
        for e in note.frontmatter.iter().flat_map(|f| f.entries()) {
            let Some(Field { field, editions }) = row(&e.key, ctx.mark) else { continue };
            let Some(update) = editions.until else { continue };
            let change = FmChange::RemoveField(*field);
            let fix = Fix::new(Applicability::Unsafe, format!("delete `{}`", *e.key)).frontmatter(&note.path, change);
            let code = Code::Update(update);
            out.push(
                Diagnostic::warning(code, format!("vault update {update} removes the concept field `{}`", *e.key))
                    .in_note(note, e.key.span.clone())
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
            fix = fix.frontmatter(&n.path, FmChange::SetId(Ulid::generate()));
        }
        let others: Vec<&str> = notes[1..].iter().map(|n| n.path.as_str()).collect();
        out.push(
            Diagnostic::error(Code::FmIdDuplicate, format!("id `{id}` is used by {}", others.join(", ")))
                .at_path(&notes[0].path)
                .with_fix(fix),
        );
    }

    for (slug, paths) in index.slugs() {
        if paths.len() > 1 {
            out.push(
                Diagnostic::error(Code::SlugDuplicate, format!("slug `{slug}` is used by {}", paths[1..].join(", ")))
                    .at_path(&paths[0]),
            );
        }
    }
}

/// `prereqs.cycle`: the cycles of the prerequisite graph that [`cycles`] finds.
pub(super) fn prereq_cycles(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    let index = ctx.index;
    let graph: BTreeMap<VaultPath, Vec<VaultPath>> = index
        .concepts()
        .map(|n| (n.path.clone(), index.prereqs(n).resolved.iter().map(|p| p.path.clone()).collect()))
        .collect();

    for cycle in cycles(&graph) {
        let shown: Vec<&str> = cycle.iter().chain(&cycle[..1]).map(VaultPath::stem).collect();
        let msg = format!("prereq cycle: {}", shown.join(" → "));
        out.push(Diagnostic::error(Code::PrereqsCycle, msg).at_path(&cycle[0]));
    }
}

/// The cycles an iterative depth-first walk of `graph` (concept path -> prerequisite
/// paths) closes, one per back edge, each once in [`canonical`] form: at least one in
/// every group of concepts that has a cycle, not every cycle.
fn cycles(graph: &BTreeMap<VaultPath, Vec<VaultPath>>) -> BTreeSet<Vec<VaultPath>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Visit {
        Unseen,
        /// On the current walk's path: an edge back to it closes a cycle.
        OnPath,
        Done,
    }

    let mut state: BTreeMap<&VaultPath, Visit> = BTreeMap::new();
    let visit = |state: &BTreeMap<&VaultPath, Visit>, p: &VaultPath| state.get(p).copied().unwrap_or(Visit::Unseen);
    let mut found = BTreeSet::new();
    for start in graph.keys() {
        if visit(&state, start) != Visit::Unseen {
            continue;
        }

        // The walk: each node on the path with the index of its next edge.
        let mut stack: Vec<(&VaultPath, usize)> = vec![(start, 0)];
        state.insert(start, Visit::OnPath);
        while let Some((node, i)) = stack.last_mut() {
            let Some(next) = graph[*node].get(*i) else {
                state.insert(node, Visit::Done);
                stack.pop();
                continue;
            };
            *i += 1;
            match visit(&state, next) {
                Visit::Unseen if graph.contains_key(next) => {
                    state.insert(next, Visit::OnPath);
                    stack.push((next, 0));
                }
                Visit::OnPath => {
                    let pos = stack.iter().position(|(p, _)| *p == next).expect("an `OnPath` node is on the path");
                    found.insert(canonical(stack[pos..].iter().map(|(p, _)| *p)));
                }
                Visit::Unseen | Visit::Done => {}
            }
        }
    }
    found
}

/// `cycle` rotated to start at its member with the smallest slug (members are reached
/// by slug, so their slugs differ), so each cycle has one form however the walk entered
/// it.
fn canonical<'p>(cycle: impl Iterator<Item = &'p VaultPath>) -> Vec<VaultPath> {
    let mut cycle: Vec<VaultPath> = cycle.cloned().collect();
    let first = (0..cycle.len()).min_by_key(|&i| cycle[i].stem()).expect("a cycle has members");
    cycle.rotate_left(first);
    cycle
}
