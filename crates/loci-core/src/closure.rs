//! `loci closure` (m1-m4 §2): the prerequisite closure of a concept.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::index::Index;
use crate::vault;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub slug: String,
    pub path: String,
    /// Shortest number of `prereqs` steps from the concept.
    pub depth: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Closure {
    /// Prerequisites first; ties by slug bytes.
    pub members: Vec<Member>,
    /// (concept path naming it, raw target) for prereqs that do not resolve.
    pub unresolved: Vec<(String, String)>,
    /// True if the reachable graph has a cycle (the order breaks it by slug bytes).
    pub cyclic: bool,
}

/// The closure of concept `root` (a concept path), up to `max_depth`.
pub fn closure(index: &Index, root: &str, max_depth: Option<usize>) -> Closure {
    // Breadth-first for depths.
    let mut depth: BTreeMap<String, usize> = BTreeMap::new();
    let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut out = Closure::default();
    let mut queue = VecDeque::from([(root.to_string(), 0usize)]);
    let mut seen: BTreeSet<String> = [root.to_string()].into();
    while let Some((p, d)) = queue.pop_front() {
        if max_depth.is_some_and(|m| d >= m) {
            continue;
        }
        let (ok, bad) = index.prereqs(index.parsed(&p).expect("closure members are notes"));
        out.unresolved.extend(bad.into_iter().map(|t| (p.clone(), t)));
        for q in &ok {
            if seen.insert(q.clone()) {
                depth.insert(q.clone(), d + 1);
                queue.push_back((q.clone(), d + 1));
            }
        }
        edges.insert(p, ok);
    }
    // Topological order over the members, prerequisites first: repeatedly emit the
    // smallest-slug member whose prerequisites (within the closure) are all emitted.
    let slug = |p: &str| vault::stem(p).to_string();
    let members: BTreeSet<String> = depth.keys().cloned().collect();
    let mut emitted: BTreeSet<String> = BTreeSet::new();
    let mut order: Vec<String> = vec![];
    while emitted.len() < members.len() {
        let ready = members
            .iter()
            .filter(|m| !emitted.contains(*m))
            .filter(|m| edges.get(*m).is_none_or(|ps| ps.iter().all(|p| !members.contains(p) || emitted.contains(p))))
            .min_by_key(|m| slug(m));
        let next = match ready {
            Some(m) => m.clone(),
            None => {
                // A cycle: break it at the smallest remaining slug.
                out.cyclic = true;
                members
                    .iter()
                    .filter(|m| !emitted.contains(*m))
                    .min_by_key(|m| slug(m))
                    .cloned()
                    .expect("members remain")
            }
        };
        emitted.insert(next.clone());
        order.push(next);
    }
    out.members = order.into_iter().map(|p| Member { slug: slug(&p), depth: depth[&p], path: p }).collect();
    if edges.values().flatten().any(|p| p == root) {
        out.cyclic = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::from_texts;

    fn c(prereqs: &[&str]) -> String {
        let list: Vec<String> = prereqs.iter().map(|p| format!("\"[[{p}]]\"")).collect();
        format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\nprereqs: [{}]\n---\n# T\n", list.join(", "))
    }

    #[test]
    fn order_depths_and_unresolved() {
        let i = from_texts(&[
            ("concepts/a/dijkstra.md", c(&["heap", "graph", "nope"])),
            ("concepts/a/heap.md", c(&["array"])),
            ("concepts/a/graph.md", c(&[])),
            ("concepts/a/array.md", c(&[])),
        ]);
        let cl = closure(&i, "concepts/a/dijkstra.md", None);
        let order: Vec<(&str, usize)> = cl.members.iter().map(|m| (m.slug.as_str(), m.depth)).collect();
        assert_eq!(order, vec![("array", 2), ("graph", 1), ("heap", 1)]);
        assert_eq!(cl.unresolved, vec![("concepts/a/dijkstra.md".to_string(), "nope".to_string())]);
        let shallow = closure(&i, "concepts/a/dijkstra.md", Some(1));
        assert_eq!(shallow.members.len(), 2);
        assert!(!cl.cyclic);
    }

    #[test]
    fn cycle_is_broken() {
        let i = from_texts(&[
            ("concepts/a/x.md", c(&["y"])),
            ("concepts/a/y.md", c(&["z"])),
            ("concepts/a/z.md", c(&["y"])),
        ]);
        let cl = closure(&i, "concepts/a/x.md", None);
        assert!(cl.cyclic);
        assert_eq!(cl.members.len(), 2);
    }
}
