//! `loci lint --fix` (m0 §9.2; research 18 §1.4): the diagnostics' fixes applied in
//! memory, pass after pass until none applies, then verified before anything is written.

use std::collections::BTreeMap;
use std::ops::Range;

use serde::Serialize;

use crate::config::Config;
use crate::envelope::{Applicability, Diagnostic, Fix, Refusal};
use crate::index::{self, Index};
use crate::lint;
use crate::path::VaultPath;
use crate::relink::{Change, Written};
use crate::splice::{self, Edit};

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
    /// The writes, for [`crate::relink::apply`].
    pub change: Change,
    pub applied: Vec<Applied>,
    /// The index rules' diagnostics after the fixes.
    pub remaining: Vec<Diagnostic>,
}

/// Plans `--fix` over `index`: applies the fixes `opts` allow, pass after pass, then
/// verifies that no error code gained diagnostics and that every link that resolved
/// still reaches the same file. Refuses (`lint.fix-verify`) when the check fails;
/// nothing is written either way.
pub fn plan(index: &Index, config: &Config, opts: &Options) -> Result<Plan, Refusal> {
    let mut texts: BTreeMap<VaultPath, String> = index.notes().map(|n| (n.path.clone(), n.text.clone())).collect();
    let mut edits: BTreeMap<VaultPath, usize> = BTreeMap::new();
    let before = lint::run(index, config);
    let mut applied = vec![];
    let mut current = index.clone();
    let mut diags = before.clone();
    for _ in 0..PASSES {
        let done = apply_pass(&mut texts, &mut edits, &diags, opts)?;
        if done.is_empty() {
            break;
        }
        applied.extend(done);
        current = index::build(index.files.clone(), |p| texts[p].clone());
        diags = lint::run(&current, config);
    }
    verify(index, &before, &current, &diags)?;
    let mut change = Change::default();
    for (p, n) in edits {
        let text = texts.remove(&p).expect("only notes are edited");
        if index.parsed(&p).is_some_and(|n| n.text != text) {
            change.writes.insert(p, Written { text, edits: n });
        }
    }
    Ok(Plan { change, applied, remaining: diags })
}

/// Applies one pass of fixes to `texts` (counting edits per file in `edits`): every
/// allowed fix of `diags` that neither overlaps nor touches one taken before it.
/// Returns what it applied.
fn apply_pass(
    texts: &mut BTreeMap<VaultPath, String>,
    edits: &mut BTreeMap<VaultPath, usize>,
    diags: &[Diagnostic],
    opts: &Options,
) -> Result<Vec<Applied>, Refusal> {
    let mut taken: BTreeMap<VaultPath, Vec<Range<usize>>> = BTreeMap::new();
    let mut out = vec![];
    let mut by_file: BTreeMap<VaultPath, Vec<Edit>> = BTreeMap::new();
    for (d, fix) in diags.iter().filter_map(|d| opts.pick(d).map(|f| (d, f))) {
        let fix_edits = fix_edits(texts, fix)?;
        // Edits that touch or overlap one already taken wait for the next pass.
        let clashes = fix_edits.iter().any(|(p, e)| {
            taken.get(p).is_some_and(|rs| rs.iter().any(|r| r.start <= e.range.end && e.range.start <= r.end))
        });
        if clashes {
            continue;
        }
        for (p, e) in fix_edits {
            taken.entry(p.clone()).or_default().push(e.range.clone());
            by_file.entry(p).or_default().push(e);
        }
        out.push(Applied { code: d.code.clone(), path: d.path().map(str::to_string), message: fix.message.clone() });
    }
    for (p, file_edits) in by_file {
        *edits.entry(p.clone()).or_default() += file_edits.len();
        let text = splice::apply(&texts[&p], file_edits).map_err(|e| Refusal::internal("lint.fix", e))?;
        texts.insert(p, text);
    }
    Ok(out)
}

/// A fix's edits by vault path. Lint made them from these texts, so an edit naming
/// another file or a range outside its text is a loci bug (`lint.fix`).
fn fix_edits(texts: &BTreeMap<VaultPath, String>, fix: &Fix) -> Result<Vec<(VaultPath, Edit)>, Refusal> {
    let bug = |m: String| Refusal::internal("lint.fix", m);
    fix.edits
        .iter()
        .map(|e| {
            let p = VaultPath::new(e.path.clone()).ok_or_else(|| bug(format!("`{}` is not a vault path", e.path)))?;
            let text = texts.get(&p).ok_or_else(|| bug(format!("a fix edits `{p}`, which is not a note")))?;
            let ok = e.byte_start <= e.byte_end
                && e.byte_end <= text.len()
                && text.is_char_boundary(e.byte_start)
                && text.is_char_boundary(e.byte_end);
            if !ok {
                return Err(bug(format!(
                    "a fix edits bytes {}..{} of `{p}`, outside its text",
                    e.byte_start, e.byte_end
                )));
            }
            Ok((p, Edit::replace(e.byte_start..e.byte_end, e.new.clone())))
        })
        .collect()
}

/// Refuses (`lint.fix-verify`) when an error code has more diagnostics after the fixes
/// than before, or when a link that resolved before reaches a different file after.
fn verify(before: &Index, was: &[Diagnostic], after: &Index, now: &[Diagnostic]) -> Result<(), Refusal> {
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
    let targets = |idx: &Index, n: &crate::parse::Note| -> Vec<VaultPath> {
        n.links.iter().filter_map(|l| idx.link_target(n, l)).collect()
    };
    for note in before.notes() {
        // Every target before is still a target after (fixes may add links).
        let mut have = after.parsed(&note.path).map_or(vec![], |n| targets(after, n));
        for t in targets(before, note) {
            match have.iter().position(|h| *h == t) {
                Some(i) => {
                    have.swap_remove(i);
                }
                None => return Err(refuse(format!("a link in {} would no longer reach {t}", note.path))),
            }
        }
    }
    Ok(())
}
