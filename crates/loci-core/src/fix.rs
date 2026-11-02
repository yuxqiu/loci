//! `loci lint --fix` (m0 §9.2; research 18 §1.4): the diagnostics' fixes applied in
//! memory, pass after pass until none applies, then verified before anything is written.

use std::collections::BTreeMap;
use std::ops::Range;

use serde::Serialize;

use crate::config::Config;
use crate::envelope::{Applicability, Diagnostic, Fix, Refusal};
use crate::index::{self, Index};
use crate::path::VaultPath;
use crate::relink::{Change, Written};
use crate::splice::{self, Edit};
use crate::{lint, mv};

/// At most this many passes (ESLint's limit); a fix that keeps applying stops here.
const PASSES: usize = 10;

/// Which fixes to apply.
pub struct Options<'a> {
    /// Unsafe fixes too, not only safe ones. Display-only fixes are never applied.
    pub unsafe_fixes: bool,
    /// Only the fixes of diagnostics with this code.
    pub only: Option<&'a str>,
}

impl Options<'_> {
    /// The fix of `d` to apply, if any: its first fix these options allow.
    fn pick<'d>(&self, d: &'d Diagnostic) -> Option<&'d Fix> {
        if self.only.is_some_and(|c| c != d.code) {
            return None;
        }
        d.fixes.iter().find(|f| match f.applicability {
            Applicability::Safe => true,
            Applicability::Unsafe => self.unsafe_fixes,
            Applicability::DisplayOnly => false,
        })
    }
}

/// A fix that was applied.
#[derive(Debug, Clone, Serialize)]
pub struct Applied {
    pub code: String,
    /// The file the diagnostic was about, as it was named when the fix applied.
    pub path: Option<String>,
    pub message: String,
}

/// What `--fix` would do.
#[derive(Debug)]
pub struct Plan {
    /// The renames and writes, for [`crate::relink::apply`].
    pub change: Change,
    pub applied: Vec<Applied>,
    /// The index rules' diagnostics after the fixes.
    pub remaining: Vec<Diagnostic>,
}

/// The vault as the fixes change it, in memory.
struct Tree {
    files: Vec<VaultPath>,
    /// The text of every markdown file, by its current path.
    texts: BTreeMap<VaultPath, String>,
    /// Current path -> original path, for files that moved.
    origin: BTreeMap<VaultPath, VaultPath>,
    /// Edits made to each file, by current path.
    edits: BTreeMap<VaultPath, usize>,
}

impl Tree {
    fn index(&self) -> Index {
        index::build(self.files.clone(), |p| self.texts[p].clone())
    }

    fn original<'a>(&'a self, p: &'a VaultPath) -> &'a VaultPath {
        self.origin.get(p).unwrap_or(p)
    }
}

/// Plans `--fix` over `index`: applies the fixes `opts` allow, pass after pass, then
/// verifies that no error code gained diagnostics and that every link that resolved
/// still reaches the same file (followed through renames). A rename goes through the
/// `mv` planner with `ignored` and `exists` (a path on disk the index does not hold).
/// Refuses (`lint.fix-verify`) when the check fails; nothing is written either way.
pub fn plan(
    index: &Index,
    config: &Config,
    opts: &Options,
    ignored: impl Fn(&VaultPath) -> Result<bool, Refusal>,
    exists: impl Fn(&VaultPath) -> bool,
) -> Result<Plan, Refusal> {
    let mut tree = Tree {
        files: index.files.clone(),
        texts: index.notes().map(|n| (n.path.clone(), n.text.clone())).collect(),
        origin: BTreeMap::new(),
        edits: BTreeMap::new(),
    };
    let before = lint::run(index, config);
    let mut applied = vec![];
    let mut current = index.clone();
    let mut diags = before.clone();
    for _ in 0..PASSES {
        let done = apply_pass(&mut tree, &current, &diags, opts, &ignored, &exists)?;
        if done.is_empty() {
            break;
        }
        applied.extend(done);
        current = tree.index();
        diags = lint::run(&current, config);
    }
    verify(index, &before, &tree, &current, &diags)?;
    let mut change = Change::default();
    for (now, was) in &tree.origin {
        change.moves.insert(was.clone(), now.clone());
    }
    for (p, n) in &tree.edits {
        let was = index.parsed(tree.original(p)).map(|n| n.text.as_str());
        if was != Some(tree.texts[p].as_str()) {
            change.writes.insert(p.clone(), Written { text: tree.texts[p].clone(), edits: *n });
        }
    }
    Ok(Plan { change, applied, remaining: diags })
}

/// Applies one pass of fixes to `tree`, whose index is `index` and whose diagnostics
/// are `diags`: every allowed text fix that overlaps no other, or else the first
/// allowed rename alone (a rename changes paths under the other fixes). Returns what
/// it applied.
fn apply_pass(
    tree: &mut Tree,
    index: &Index,
    diags: &[Diagnostic],
    opts: &Options,
    ignored: &impl Fn(&VaultPath) -> Result<bool, Refusal>,
    exists: &impl Fn(&VaultPath) -> bool,
) -> Result<Vec<Applied>, Refusal> {
    let applied = |d: &Diagnostic, f: &Fix| Applied {
        code: d.code.clone(),
        path: d.path().map(str::to_string),
        message: f.message.clone(),
    };
    let mut taken: BTreeMap<VaultPath, Vec<Range<usize>>> = BTreeMap::new();
    let mut out = vec![];
    for (d, fix) in diags.iter().filter_map(|d| opts.pick(d).map(|f| (d, f))).filter(|(_, f)| f.moves.is_empty()) {
        let Some(edits) = fix_edits(tree, fix) else { continue };
        // Edits that touch or overlap one already taken wait for the next pass.
        let clashes = edits.iter().any(|(p, e)| {
            taken.get(p).is_some_and(|rs| rs.iter().any(|r| r.start <= e.range.end && e.range.start <= r.end))
        });
        if clashes {
            continue;
        }
        for (p, e) in &edits {
            taken.entry(p.clone()).or_default().push(e.range.clone());
        }
        out.push((applied(d, fix), edits));
    }
    if out.is_empty() {
        let Some((d, fix)) =
            diags.iter().filter_map(|d| opts.pick(d).map(|f| (d, f))).find(|(_, f)| !f.moves.is_empty())
        else {
            return Ok(vec![]);
        };
        for m in &fix.moves {
            let (from, to) = (path(&m.from)?, path(&m.to)?);
            let change = mv::plan(index, &from, &to, ignored, |p| index.contains(p) || exists(p))?;
            rename(tree, &change);
        }
        return Ok(vec![applied(d, fix)]);
    }
    let mut by_file: BTreeMap<VaultPath, Vec<Edit>> = BTreeMap::new();
    for (_, edits) in &out {
        for (p, e) in edits {
            by_file.entry(p.clone()).or_default().push(e.clone());
        }
    }
    for (p, edits) in by_file {
        *tree.edits.entry(p.clone()).or_default() += edits.len();
        let text = splice::apply(&tree.texts[&p], edits).map_err(|e| Refusal::internal("lint.fix", e.to_string()))?;
        tree.texts.insert(p, text);
    }
    Ok(out.into_iter().map(|(a, _)| a).collect())
}

/// A fix's edits by vault path, or `None` when one names a file that is not a note of
/// the tree or a range outside it (the fix is skipped).
fn fix_edits(tree: &Tree, fix: &Fix) -> Option<Vec<(VaultPath, Edit)>> {
    fix.edits
        .iter()
        .map(|e| {
            let p = VaultPath::new(e.path.clone())?;
            let text = tree.texts.get(&p)?;
            let ok = e.byte_start <= e.byte_end
                && e.byte_end <= text.len()
                && text.is_char_boundary(e.byte_start)
                && text.is_char_boundary(e.byte_end);
            ok.then(|| (p, Edit::replace(e.byte_start..e.byte_end, e.new.clone())))
        })
        .collect()
}

fn path(s: &str) -> Result<VaultPath, Refusal> {
    VaultPath::new(s.to_string()).ok_or_else(|| Refusal::internal("lint.fix", format!("`{s}` is not a vault path")))
}

/// Applies a planned rename to `tree`: moved files take their new paths, then the link
/// edits are written.
fn rename(tree: &mut Tree, change: &Change) {
    for (old, new) in &change.moves {
        let was = tree.origin.remove(old).unwrap_or_else(|| old.clone());
        if was != *new {
            tree.origin.insert(new.clone(), was);
        }
        if let Some(text) = tree.texts.remove(old) {
            tree.texts.insert(new.clone(), text);
        }
        if let Some(n) = tree.edits.remove(old) {
            tree.edits.insert(new.clone(), n);
        }
        let i = tree.files.binary_search(old).expect("`mv` moves listed files");
        tree.files.remove(i);
        let j = tree.files.binary_search(new).expect_err("`mv` refuses an existing destination");
        tree.files.insert(j, new.clone());
    }
    for (p, w) in &change.writes {
        tree.texts.insert(p.clone(), w.text.clone());
        *tree.edits.entry(p.clone()).or_default() += w.edits;
    }
}

/// Refuses (`lint.fix-verify`) when an error code has more diagnostics after the fixes
/// than before, or when a link that resolved before reaches a different file after.
fn verify(before: &Index, was: &[Diagnostic], tree: &Tree, after: &Index, now: &[Diagnostic]) -> Result<(), Refusal> {
    let refuse = |m: String| Refusal::refused("lint.fix-verify", format!("{m}; nothing was written"));
    let errors = |ds: &[Diagnostic]| {
        let mut n: BTreeMap<String, usize> = BTreeMap::new();
        for d in ds.iter().filter(|d| d.is_error()) {
            *n.entry(d.code.clone()).or_default() += 1;
        }
        n
    };
    let (was_n, now_n) = (errors(was), errors(now));
    if let Some((code, n)) = now_n.iter().find(|(c, n)| **n > was_n.get(*c).copied().unwrap_or(0)) {
        let w = was_n.get(code).copied().unwrap_or(0);
        return Err(refuse(format!("the fixes would raise the `{code}` errors from {w} to {n}")));
    }
    let current: BTreeMap<&VaultPath, &VaultPath> = tree.origin.iter().map(|(now, was)| (was, now)).collect();
    let moved = |p: VaultPath| current.get(&p).map_or(p.clone(), |n| (*n).clone());
    for note in before.notes() {
        let targets = |idx: &Index, n: &crate::parse::Note| {
            let mut t: Vec<VaultPath> = n.links.iter().filter_map(|l| idx.link_target(n, l)).collect();
            t.sort();
            t
        };
        let want: Vec<VaultPath> = targets(before, note).into_iter().map(&moved).collect();
        let new_path = moved(note.path.clone());
        let have = after.parsed(&new_path).map_or(vec![], |n| targets(after, n));
        // Every target before is still a target after (fixes may add links).
        let mut rest = have.clone();
        for t in &want {
            match rest.iter().position(|h| h == t) {
                Some(i) => {
                    rest.remove(i);
                }
                None => return Err(refuse(format!("a link in {new_path} would no longer reach {t}"))),
            }
        }
    }
    Ok(())
}
