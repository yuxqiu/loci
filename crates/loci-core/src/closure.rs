//! `loci closure` (m1-m4 §2): the prerequisite closure of a concept.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::id::Slug;
use crate::index::Index;
use crate::parse::Note;
use crate::path::VaultPath;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The concept's path; its slug is the path's stem.
    pub path: VaultPath,
    /// Shortest number of `prereqs` steps from the concept.
    pub depth: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Closure {
    /// Prerequisites first; ties by slug bytes.
    pub members: Vec<Member>,
    /// (concept path naming it, the slug) for prereqs that do not resolve.
    pub unresolved: Vec<(VaultPath, Slug)>,
    /// True if the members, or the members and the concept, form a cycle (the order
    /// breaks it by slug bytes).
    pub cyclic: bool,
}

impl Closure {
    /// The closure of concept `root`, up to `max_depth`.
    pub fn of(index: &Index, root: &Note, max_depth: Option<usize>) -> Closure {
        // Breadth-first for depths.
        let root_path = &root.path;
        let mut depths: BTreeMap<VaultPath, usize> = BTreeMap::new();
        let mut edges: BTreeMap<VaultPath, Vec<VaultPath>> = BTreeMap::new();
        let mut out = Closure::default();
        let mut queue = VecDeque::from([(root, 0usize)]);
        while let Some((note, d)) = queue.pop_front() {
            // A member at the depth limit keeps its edges, for the order and cycles.
            let prereqs = index.prereqs(note);
            edges.insert(note.path.clone(), prereqs.resolved.iter().map(|q| q.path.clone()).collect());
            if max_depth.is_some_and(|m| d >= m) {
                continue;
            }
            out.unresolved.extend(prereqs.unresolved.into_iter().map(|s| (note.path.clone(), s.clone())));
            for q in &prereqs.resolved {
                if q.path != *root_path && !depths.contains_key(&q.path) {
                    depths.insert(q.path.clone(), d + 1);
                    queue.push_back((q, d + 1));
                }
            }
        }

        let (order, cycle_broken) = prerequisites_first(&depths, &edges);
        out.members = order.into_iter().map(|p| Member { depth: depths[&p], path: p }).collect();
        // A prereq back to the root is a cycle too, though the root is no member.
        out.cyclic = cycle_broken || edges.values().flatten().any(|p| p == root_path);
        out
    }
}

/// The members (`depths`' keys) in topological order, prerequisites first: repeatedly
/// the smallest-slug member whose prerequisites (within the closure) are all placed.
/// When none is ready there is a cycle, broken at the smallest remaining slug; the
/// second value says whether that happened.
fn prerequisites_first(
    depths: &BTreeMap<VaultPath, usize>,
    edges: &BTreeMap<VaultPath, Vec<VaultPath>>,
) -> (Vec<VaultPath>, bool) {
    let mut placed: BTreeSet<&VaultPath> = BTreeSet::new();
    let mut order: Vec<VaultPath> = vec![];
    let mut cycle_broken = false;
    while placed.len() < depths.len() {
        let remaining = || depths.keys().filter(|m| !placed.contains(m));
        let ready = remaining()
            .filter(|m| edges.get(*m).is_none_or(|ps| ps.iter().all(|p| !depths.contains_key(p) || placed.contains(p))))
            .min_by_key(|m| m.stem());
        let next = ready.unwrap_or_else(|| {
            cycle_broken = true;
            remaining().min_by_key(|m| m.stem()).expect("members remain")
        });
        placed.insert(next);
        order.push(next.clone());
    }
    (order, cycle_broken)
}
