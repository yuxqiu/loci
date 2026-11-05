//! `loci lint --fix` (m0 §9.2; research 18 §1.4): the diagnostics' fixes applied in
//! memory, pass after pass until none applies, then verified before anything is written.

use std::collections::BTreeMap;
use std::ops::Range;

use serde::Serialize;

use crate::envelope::{Applicability, Diagnostic, Fix, FixEdit, Refusal};
use crate::index::Index;
use crate::lint::{self, Code};
use crate::parse::{FmChange, Note};
use crate::path::VaultPath;
use crate::relink::{Change, Edited};
use crate::splice::{self, Edit};
use crate::vault::Vault;

/// At most this many passes (ESLint's limit); a fix that keeps applying stops here.
pub const PASSES: usize = 10;

/// Which fixes to apply.
#[derive(Debug)]
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
    pub fn new(index: &Index, opts: &Options) -> Result<Plan, Refusal> {
        Self::with_passes(index, opts, PASSES)
    }

    /// [`Plan::new`] with at most `passes` passes.
    fn with_passes(index: &Index, opts: &Options, passes: usize) -> Result<Plan, Refusal> {
        let mut notes: BTreeMap<VaultPath, Edited> =
            index.notes().map(|n| (n.path.clone(), Edited { text: n.text.clone(), edits: 0 })).collect();
        let was = lint::run(index);
        let mut applied = vec![];
        let mut passes_left = passes;
        // The tree as fixed by the passes so far, once one applied a fix.
        let mut fixed: Option<Vault> = None;
        loop {
            let reindexed = fixed.as_ref().map(Index::open);
            let current = reindexed.as_ref().unwrap_or(index);
            let diags = reindexed.as_ref().map_or_else(|| was.clone(), lint::run);
            let this_pass = if passes_left == 0 {
                vec![]
            } else {
                passes_left -= 1;
                apply_pass(current, &mut notes, &diags, opts)?
            };
            if this_pass.is_empty() {
                verify(index, &was, current, &diags)?;
                let changed = |p: &VaultPath, e: &Edited| index.note_at(p).is_some_and(|n| n.text != e.text);
                let writes = notes.into_iter().filter(|(p, e)| changed(p, e)).collect();
                // A pass with any fix to apply applies one, so fixes are left only when
                // the passes ran out.
                let unapplied = diags.iter().filter(|d| opts.pick(d).is_some()).count();
                let change = Change { writes, ..Change::default() };
                return Ok(Plan { change, applied, remaining: diags, unapplied });
            }
            applied.extend(this_pass);
            fixed = Some(index.vault().after(&Change { writes: notes.clone(), ..Change::default() }));
        }
    }
}

/// Applies one pass of fixes to `notes`, the notes of `index` as edited so far: every
/// allowed fix of `diags` that neither overlaps nor touches one taken before it. A fix
/// whose edits were all taken already (another diagnostic's copy of it) is applied by
/// them. Returns what it applied.
fn apply_pass(
    index: &Index,
    notes: &mut BTreeMap<VaultPath, Edited>,
    diags: &[Diagnostic],
    opts: &Options,
) -> Result<Vec<Applied>, Refusal> {
    let mut pass = Pass::default();
    let mut out = vec![];
    for (d, fix) in diags.iter().filter_map(|d| opts.pick(d).map(|f| (d, f))) {
        let planned = planned(index, fix)?;
        if !pass.took_all(&planned) {
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

/// The edits one pass has taken, by note: text edits, and frontmatter changes, which
/// compose (each note's are applied as one rewrite of its block, which only text edits
/// can touch).
#[derive(Default)]
struct Pass<'a> {
    text: BTreeMap<VaultPath, Vec<Edit>>,
    frontmatter: BTreeMap<VaultPath, (&'a Note, Vec<FmChange>)>,
}

impl<'a> Pass<'a> {
    /// Whether every edit of `planned` is taken already.
    fn took_all(&self, planned: &[(&Note, Planned)]) -> bool {
        planned.iter().all(|(n, e)| match e {
            Planned::Text(e) => self.text.get(&n.path).is_some_and(|es| es.contains(e)),
            Planned::Frontmatter(c) => self.frontmatter.get(&n.path).is_some_and(|(_, cs)| cs.contains(c)),
        })
    }

    /// Whether an edit of `planned` touches one taken: a text edit touching another, or
    /// the bytes a rewrite of a note's frontmatter may replace.
    fn clashes(&self, planned: &[(&Note, Planned)]) -> bool {
        let touch = |a: &Range<usize>, b: &Range<usize>| a.start <= b.end && b.start <= a.end;
        planned.iter().any(|(n, e)| {
            let taken = self.text.get(&n.path).map_or(&[][..], Vec::as_slice);
            match e {
                Planned::Text(e) => {
                    taken.iter().any(|t| touch(&t.range, &e.range))
                        || (self.frontmatter.contains_key(&n.path) && touch(&n.frontmatter_reach(), &e.range))
                }
                Planned::Frontmatter(_) => taken.iter().any(|t| touch(&t.range, &n.frontmatter_reach())),
            }
        })
    }

    fn take(&mut self, planned: Vec<(&'a Note, Planned)>) {
        for (n, e) in planned {
            match e {
                Planned::Text(e) => self.text.entry(n.path.clone()).or_default().push(e),
                Planned::Frontmatter(c) => self.frontmatter.entry(n.path.clone()).or_insert((n, vec![])).1.push(c),
            }
        }
    }

    /// Applies the pass to `notes`, counting its edits.
    fn apply(mut self, notes: &mut BTreeMap<VaultPath, Edited>) -> Result<(), Refusal> {
        // A note's frontmatter changes are one rewrite, so one edit.
        for (p, (note, cs)) in std::mem::take(&mut self.frontmatter) {
            self.text.entry(p).or_default().push(note.rewrite_frontmatter(&cs));
        }

        for (p, file_edits) in self.text {
            let edited = notes.get_mut(&p).expect("only notes are edited");
            edited.edits += file_edits.len();
            edited.text = splice::apply(&edited.text, file_edits).map_err(|e| Refusal::internal("lint.fix", e))?;
        }
        Ok(())
    }
}

/// A fix's edit, checked against the note it edits.
enum Planned {
    Text(Edit),
    Frontmatter(FmChange),
}

/// A fix's edits, each with the note it edits. Lint made them from `index`, so an edit
/// naming another file or a broken frontmatter is a loci bug (`lint.fix`); so is a range
/// outside its text, which `splice::apply` refuses.
fn planned<'i>(index: &'i Index, fix: &Fix) -> Result<Vec<(&'i Note, Planned)>, Refusal> {
    let bug = |m: String| Refusal::internal("lint.fix", m);
    let note = |p: &VaultPath| index.note_at(p).ok_or_else(|| bug(format!("a fix edits `{p}`, which is not a note")));

    fix.edits
        .iter()
        .map(|e| match e {
            FixEdit::Text(e) => {
                Ok((note(&e.path)?, Planned::Text(Edit::replace(e.byte_start..e.byte_end, e.new.clone()))))
            }
            FixEdit::Frontmatter { path, frontmatter } => {
                let n = note(path)?;
                if n.has_broken_frontmatter() {
                    return Err(bug(format!("a fix changes the broken frontmatter of `{path}`")));
                }
                Ok((n, Planned::Frontmatter(frontmatter.clone())))
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
    let was_errors = |code: &str| was_n.get(code).copied().unwrap_or(0);
    if let Some((code, n)) = now_n.iter().find(|(c, n)| **n > was_errors(c)) {
        return Err(refuse(format!("the fixes would raise the `{code}` errors from {} to {n}", was_errors(code))));
    }

    // Markdown links count too: a fix that turns one into a wikilink keeps its target,
    // and one that loses it is caught.
    let targets = |idx: &Index, n: &Note| -> Vec<VaultPath> {
        let links = n.links.iter().filter_map(|l| idx.link_target(n, l));
        let markdown = n.markdown_links.iter().filter_map(|m| idx.vault().resolve(m.path()));
        links.chain(markdown).cloned().collect()
    };

    for note in before.notes() {
        // Every target before is still a target after (fixes may add links).
        let mut have = after.note_at(&note.path).map_or(vec![], |n| targets(after, n));
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
    fn a_frontmatter_rewrite_is_one_edit() {
        let index = crate::testing::index(&[
            ("concepts/a/x.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n---\n# X\n"),
            ("concepts/a/y.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TW\nprereqs:\n  - [[x]]\n---\n# Y\n"),
        ]);
        let plan = Plan::new(&index, &Options { unsafe_fixes: false, only: None }).unwrap();
        assert_eq!(
            plan.change.writes.get("concepts/a/y.md").map(|w| w.edits),
            Some(1),
            "{:?} {:?}",
            plan.applied,
            plan.remaining
        );
    }

    #[test]
    fn verification_refuses_a_fix_that_loses_a_link() {
        let w = ("views/w.md", "# W\n");
        let before = crate::testing::index(&[w, ("views/v.md", "# V\n\n[[views/w]] [w](views/w.md)\n")]);
        for v in ["# V\n\n[[views/w]] gone\n", "# V\n\ngone [w](views/w.md)\n"] {
            let after = crate::testing::index(&[w, ("views/v.md", v)]);
            let refused = verify(&before, &[], &after, &[]).unwrap_err();
            assert!(refused.error.message.contains("would no longer reach views/w.md"), "{v:?}: {refused:?}");
        }
        let kept = crate::testing::index(&[w, ("views/v.md", "# V\n\n[[views/w]] [[views/w|w]]\n")]);
        assert!(verify(&before, &[], &kept, &[]).is_ok(), "a markdown link become a wikilink keeps its target");
    }

    #[test]
    fn fixes_left_when_the_passes_run_out_are_counted() {
        // Two adjacent markdown links: their fixes touch, so one applies per pass.
        let index = crate::testing::index(&[
            ("concepts/a/x.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n---\n# X\n"),
            ("views/v.md", "# V\n\n[x](concepts/a/x.md)[x](concepts/a/x.md)\n"),
        ]);
        let opts = Options { unsafe_fixes: false, only: Some(Code::LinkMarkdown) };
        let one = Plan::with_passes(&index, &opts, 1).unwrap();
        assert_eq!((one.applied.len(), one.unapplied), (1, 1));
        let all = Plan::new(&index, &opts).unwrap();
        assert_eq!((all.applied.len(), all.unapplied), (2, 0));
    }
}
