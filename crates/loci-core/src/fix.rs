//! `loci lint --fix` (m0 §9.2; research 18 §1.4): the diagnostics' fixes applied in
//! memory, pass after pass until none applies, then verified before anything is written.

use std::collections::BTreeMap;
use std::ops::Range;

use serde::Serialize;

use crate::config::Config;
use crate::envelope::{Applicability, Diagnostic, Fix, FixEdit, Refusal};
use crate::index::Index;
use crate::lint::{self, Code};
use crate::parse::FmChange;
use crate::path::VaultPath;
use crate::relink::{Change, Written};
use crate::splice::{self, Edit};

/// At most this many passes (ESLint's limit); a fix that keeps applying stops here.
pub const PASSES: usize = 10;

/// Which fixes to apply.
pub struct Options {
    /// Unsafe fixes too, not only safe ones. Display-only fixes are never applied.
    pub unsafe_fixes: bool,
    /// Only the fixes of diagnostics with this code.
    pub only: Option<Code>,
}

impl Options {
    /// The fix of `d` to apply, if any: its first fix these options allow.
    fn pick<'d>(&self, d: &'d Diagnostic) -> Option<&'d Fix> {
        if self.only.is_some_and(|c| d.code.parse() != Ok(c)) {
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
    pub path: Option<VaultPath>,
    pub message: String,
}

/// What `--fix` would do.
#[derive(Debug)]
pub struct Plan {
    /// The writes, for [`Change::apply`].
    pub change: Change,
    pub applied: Vec<Applied>,
    /// The index rules' diagnostics after the fixes.
    pub remaining: Vec<Diagnostic>,
    /// How many of `remaining` still have a fix to apply: none unless the passes ran out.
    pub unapplied: usize,
}

impl Plan {
    /// Plans `--fix` over `index`: applies the fixes `opts` allow, pass after pass, then
    /// verifies that no error code gained diagnostics and that every link that resolved
    /// still reaches the same file. Refuses (`lint.fix-verify`) when the check fails;
    /// nothing is written either way.
    pub fn new(index: &Index, config: &Config, opts: &Options) -> Result<Plan, Refusal> {
        Self::with_passes(index, config, opts, PASSES)
    }

    /// [`Plan::new`] with at most `passes` passes.
    fn with_passes(index: &Index, config: &Config, opts: &Options, passes: usize) -> Result<Plan, Refusal> {
        let mut notes: BTreeMap<VaultPath, Written> =
            index.notes().map(|n| (n.path.clone(), Written { text: n.text.clone(), edits: 0 })).collect();
        let before = lint::run(index, config);
        let mut applied = vec![];
        let mut current = index.clone();
        let mut diags = before.clone();
        for _ in 0..passes {
            let done = apply_pass(&current, &mut notes, &diags, opts)?;
            if done.is_empty() {
                break;
            }
            applied.extend(done);
            current = Index::build(index.files().to_vec(), |p| notes[p].text.clone());
            diags = lint::run(&current, config);
        }

        verify(index, &before, &current, &diags)?;
        let changed = |p: &VaultPath, w: &Written| index.parsed(p).is_some_and(|n| n.text != w.text);
        let change = Change { writes: notes.into_iter().filter(|(p, w)| changed(p, w)).collect(), ..Change::default() };

        // A pass with any fix to apply applies one, so fixes are left only when the passes
        // ran out.
        let unapplied = diags.iter().filter(|d| opts.pick(d).is_some()).count();
        Ok(Plan { change, applied, remaining: diags, unapplied })
    }
}

/// Applies one pass of fixes to `notes`, the notes of `index` as edited so far: every
/// allowed fix of `diags` that neither overlaps nor touches one
/// taken before it. A fix whose edits were all taken already (another diagnostic's copy
/// of it) is applied by them. Returns what it applied.
fn apply_pass(
    index: &Index,
    notes: &mut BTreeMap<VaultPath, Written>,
    diags: &[Diagnostic],
    opts: &Options,
) -> Result<Vec<Applied>, Refusal> {
    let mut pass = Pass { index, text: BTreeMap::new(), frontmatter: BTreeMap::new() };
    let mut out = vec![];
    for (d, fix) in diags.iter().filter_map(|d| opts.pick(d).map(|f| (d, f))) {
        let planned = planned(index, fix)?;
        if !pass.holds(&planned) {
            // Edits that touch one taken wait for the next pass.
            if pass.clashes(&planned) {
                continue;
            }
            pass.take(planned);
        }
        out.push(Applied { code: d.code.clone(), path: d.path().cloned(), message: fix.message.clone() });
    }
    pass.apply(notes)?;
    Ok(out)
}

/// The edits one pass has taken: text edits, and frontmatter changes, which compose
/// (each note's are applied as one rewrite of its block, which only text edits can
/// touch).
struct Pass<'a> {
    index: &'a Index,
    text: BTreeMap<VaultPath, Vec<Edit>>,
    frontmatter: BTreeMap<VaultPath, Vec<FmChange>>,
}

impl Pass<'_> {
    /// Whether every edit of `planned` is taken already.
    fn holds(&self, planned: &[(VaultPath, Planned)]) -> bool {
        planned.iter().all(|(p, e)| match e {
            Planned::Text(e) => self.text.get(p).is_some_and(|es| es.contains(e)),
            Planned::Frontmatter(c) => self.frontmatter.get(p).is_some_and(|cs| cs.contains(c)),
        })
    }

    /// Whether an edit of `planned` touches one taken: a text edit touching another, or
    /// the block of a note whose frontmatter changes.
    fn clashes(&self, planned: &[(VaultPath, Planned)]) -> bool {
        let touch = |a: &Range<usize>, b: &Range<usize>| a.start <= b.end && b.start <= a.end;
        planned.iter().any(|(p, e)| {
            let taken = self.text.get(p).map_or(&[][..], Vec::as_slice);
            match e {
                Planned::Text(e) => {
                    taken.iter().any(|t| touch(&t.range, &e.range))
                        || (self.frontmatter.contains_key(p) && touch(&self.block(p), &e.range))
                }
                Planned::Frontmatter(_) => taken.iter().any(|t| touch(&t.range, &self.block(p))),
            }
        })
    }

    /// The bytes a rewrite of `path`'s frontmatter may replace.
    fn block(&self, path: &VaultPath) -> Range<usize> {
        self.index.parsed(path).expect("`planned` checked the note").frontmatter_reach()
    }

    fn take(&mut self, planned: Vec<(VaultPath, Planned)>) {
        for (p, e) in planned {
            match e {
                Planned::Text(e) => self.text.entry(p).or_default().push(e),
                Planned::Frontmatter(c) => self.frontmatter.entry(p).or_default().push(c),
            }
        }
    }

    /// Applies the pass to `notes`, counting its edits.
    fn apply(mut self, notes: &mut BTreeMap<VaultPath, Written>) -> Result<(), Refusal> {
        for (p, cs) in std::mem::take(&mut self.frontmatter) {
            notes.get_mut(&p).expect("only notes are edited").edits += cs.len();
            let note = self.index.parsed(&p).expect("`planned` checked the note");
            self.text.entry(p).or_default().push(note.rewrite_frontmatter(&cs));
        }

        for (p, file_edits) in self.text {
            let written = notes.get_mut(&p).expect("only notes are edited");
            written.edits += file_edits.len();
            written.text = splice::apply(&written.text, file_edits).map_err(|e| Refusal::internal("lint.fix", e))?;
        }
        Ok(())
    }
}

/// A fix's edit, checked against the note it edits.
enum Planned {
    Text(Edit),
    Frontmatter(FmChange),
}

/// A fix's edits by vault path. Lint made them from `index`, so an edit naming another
/// file or a broken frontmatter is a loci bug (`lint.fix`); so is a range outside its
/// text, which `splice::apply` refuses.
fn planned(index: &Index, fix: &Fix) -> Result<Vec<(VaultPath, Planned)>, Refusal> {
    let bug = |m: String| Refusal::internal("lint.fix", m);
    let note = |p: &VaultPath| index.parsed(p).ok_or_else(|| bug(format!("a fix edits `{p}`, which is not a note")));

    fix.edits
        .iter()
        .map(|e| match e {
            FixEdit::Text(e) => {
                note(&e.path)?;
                Ok((e.path.clone(), Planned::Text(Edit::replace(e.byte_start..e.byte_end, e.new.clone()))))
            }
            FixEdit::Frontmatter { path, frontmatter } => {
                if note(path)?.frontmatter_broken() {
                    return Err(bug(format!("a fix changes the broken frontmatter of `{path}`")));
                }
                Ok((path.clone(), Planned::Frontmatter(frontmatter.clone())))
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_left_when_the_passes_run_out_are_counted() {
        // Two adjacent markdown links: their fixes touch, so one applies per pass.
        let index = Index::from_texts(&[
            ("concepts/a/x.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n---\n# X\n"),
            ("views/v.md", "# V\n\n[x](concepts/a/x.md)[x](concepts/a/x.md)\n"),
        ]);
        let config = Config::parse("schema = 1\n").unwrap();
        let opts = Options { unsafe_fixes: false, only: Some(Code::LinkMarkdown) };
        let one = Plan::with_passes(&index, &config, &opts, 1).unwrap();
        assert_eq!((one.applied.len(), one.unapplied), (1, 1));
        let all = Plan::new(&index, &config, &opts).unwrap();
        assert_eq!((all.applied.len(), all.unapplied), (2, 0));
    }
}
