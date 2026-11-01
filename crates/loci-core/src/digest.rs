//! The index printed by `loci digest` (m0 §9.4, §10).

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::index::Index;
use crate::parse::{Note, TopBlock};
use crate::vault::{self, Kind};

/// An AREA.md that has exactly an H1 and one paragraph (m0 §6.3): (title, paragraph).
pub fn area_parts(note: &Note) -> Option<(String, String)> {
    if note.frontmatter.is_some() || note.fm_error.is_some() {
        return None;
    }
    match note.top_blocks.as_slice() {
        [(TopBlock::Heading(1), _), (TopBlock::Paragraph, p)] => {
            let title = note.headings.first()?.text.clone();
            Some((title, note.text[p.clone()].trim().to_string()))
        }
        _ => None,
    }
}

/// The lint errors of a `digest` string (m0 §7.3), as (code, message): empty, several
/// lines, a link.
pub fn digest_errors(value: &str) -> Vec<(&'static str, &'static str)> {
    if value.trim().is_empty() {
        return vec![("fm.missing-field", "concept has no `digest`")];
    }
    let mut out = vec![];
    if value.contains('\n') {
        out.push(("digest.multiline", "`digest` spans several lines"));
    }
    if value.contains("[[") {
        out.push(("digest.has-link", "`digest` contains a link"));
    }
    out
}

/// A concept's digest if it is valid for the index: a string without
/// [`digest_errors`].
pub fn valid_digest(note: &Note) -> Option<String> {
    let value = note.fm_str("digest")?;
    digest_errors(value).is_empty().then(|| value.trim().to_string())
}

/// One concept line of an area index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub slug: String,
    pub path: String,
    /// The concept's digest, if valid.
    pub digest: Option<String>,
}

pub struct Area {
    pub name: String,
    pub title: String,
    pub paragraph: Option<String>,
    /// Concepts grouped by sub-directory relative to the area ("" for direct ones),
    /// groups sorted by path bytes, concepts sorted by lowercased then raw slug.
    pub groups: Vec<(String, Vec<Entry>)>,
}

impl Area {
    pub fn count(&self) -> usize {
        self.groups.iter().map(|(_, c)| c.len()).sum()
    }
}

/// All areas of the vault, sorted by folder name bytes.
pub fn areas(index: &Index) -> Vec<Area> {
    index.areas().into_iter().map(|name| area(index, name)).collect()
}

pub fn area(index: &Index, name: &str) -> Area {
    let area_md = format!("concepts/{name}/AREA.md");
    let parts = index.notes.get(&area_md).and_then(area_parts);
    let heading_title = index.notes.get(&area_md).and_then(|n| n.h1()).map(str::to_string);
    let title = parts.as_ref().map(|(t, _)| t.clone()).or(heading_title).unwrap_or_else(|| name.to_string());
    let prefix = format!("concepts/{name}/");
    let mut groups: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for note in index.notes.values().filter(|n| n.kind == Kind::Concept) {
        let Some(rest) = note.path.strip_prefix(&prefix) else { continue };
        let dir = rest.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        let slug = vault::stem(&note.path).to_string();
        groups.entry(dir).or_default().push(Entry { slug, path: note.path.clone(), digest: valid_digest(note) });
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    for (_, cs) in &mut groups {
        cs.sort_by(|a, b| a.slug.to_lowercase().cmp(&b.slug.to_lowercase()).then(a.slug.cmp(&b.slug)));
    }
    Area { name: name.to_string(), title, paragraph: parts.map(|(_, p)| p), groups }
}

/// One concept line of a digest listing: `- [[slug]]: digest` (m0 §10).
pub fn concept_line(slug: &str, digest: Option<&str>) -> String {
    match digest {
        Some(d) => format!("- [[{slug}]]: {d}\n"),
        None => format!("- [[{slug}]]\n"),
    }
}

pub fn render_area(a: &Area) -> String {
    let mut out = format!("# {} (concepts/{})\n", a.title, a.name);
    for (dir, concepts) in &a.groups {
        out.push('\n');
        if !dir.is_empty() {
            out.push_str(&format!("## {dir}\n\n"));
        }
        for e in concepts {
            out.push_str(&concept_line(&e.slug, e.digest.as_deref()));
        }
    }
    out
}

pub fn render_root(areas: &[Area]) -> String {
    let mut out = String::from("# Index\n");
    for a in areas {
        out.push_str(&format!("\n## {} (concepts/{}, {} concepts)\n", a.title, a.name, a.count()));
        if let Some(p) = &a.paragraph {
            out.push_str(&format!("\n{p}\n"));
        }
    }
    out
}

pub fn root_json(areas: &[Area]) -> Value {
    json!({
        "areas": areas.iter().map(|a| json!({
            "area": a.name, "title": a.title, "paragraph": a.paragraph, "concepts": a.count(),
        })).collect::<Vec<_>>()
    })
}

pub fn area_json(a: &Area) -> Value {
    json!({
        "area": a.name,
        "title": a.title,
        "groups": a.groups.iter().map(|(dir, cs)| json!({
            "dir": dir,
            "concepts": cs.iter().map(|e| json!({
                "slug": e.slug, "path": e.path, "digest": e.digest,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::from_texts;

    #[test]
    fn areas_hold_a_concept_or_area_md() {
        let i = from_texts(&[
            ("concepts/a/x.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\ndigest: d\n---\n# X\n"),
            ("concepts/b/AREA.md", "# B\n\nEmpty so far.\n"),
            ("concepts/img/figure.png", ""),
        ]);
        let names: Vec<String> = areas(&i).into_iter().map(|a| a.name).collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn root_and_area() {
        let i = from_texts(&[
            ("concepts/algorithms/AREA.md", "# Algorithms\n\nTechniques for searching.\n"),
            ("concepts/algorithms/merge-sort.md", "---\ndigest: Stable sort.\n---\n# M\n"),
            ("concepts/algorithms/Binary-search.md", "---\ndigest: Find a key.\n---\n# B\n"),
            ("concepts/algorithms/graphs/bfs.md", "---\ndigest: \"\"\n---\n# BFS\n"),
            ("concepts/zeta/x.md", "# X\n"),
        ]);
        let areas = areas(&i);
        assert_eq!(
            render_root(&areas),
            "# Index\n\n## Algorithms (concepts/algorithms, 3 concepts)\n\nTechniques for searching.\n\n## zeta (concepts/zeta, 1 concepts)\n"
        );
        assert_eq!(
            render_area(&areas[0]),
            "# Algorithms (concepts/algorithms)\n\n- [[Binary-search]]: Find a key.\n- [[merge-sort]]: Stable sort.\n\n## graphs\n\n- [[bfs]]\n"
        );
    }

    #[test]
    fn empty_vault() {
        assert_eq!(render_root(&[]), "# Index\n");
    }
}
