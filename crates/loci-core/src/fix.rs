//! `loci lint --fix` (m0 §9.2; research 18 §1.4): the diagnostics' fixes applied in
//! memory, pass after pass until none applies, then verified before anything is written.

use std::collections::BTreeMap;
use std::ops::Range;

use serde::Serialize;

use crate::config::Config;
use crate::envelope::{Applicability, Diagnostic, Fix, FixEdit, Refusal};
use crate::index::{self, Index};
use crate::lint;
use crate::parse::{FmChange, rewrite_frontmatter};
use crate::path::VaultPath;
use crate::relink::{Change, Written};
use crate::splice::{self, Edit};

/// At most this many passes (ESLint's limit); a fix that keeps applying stops here.
pub const PASSES: usize = 10;

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
    /// How many of `remaining` still have a fix to apply: none unless the passes ran out.
    pub unapplied: usize,
}

/// Plans `--fix` over `index`: applies the fixes `opts` allow, pass after pass, then
/// verifies that no error code gained diagnostics and that every link that resolved
/// still reaches the same file. Refuses (`lint.fix-verify`) when the check fails;
/// nothing is written either way.
pub fn plan(index: &Index, config: &Config, opts: &Options) -> Result<Plan, Refusal> {
    plan_passes(index, config, opts, PASSES)
}

/// [`plan`] with at most `passes` passes.
fn plan_passes(index: &Index, config: &Config, opts: &Options, passes: usize) -> Result<Plan, Refusal> {
    let mut texts: BTreeMap<VaultPath, String> = index.notes().map(|n| (n.path.clone(), n.text.clone())).collect();
    let mut edits: BTreeMap<VaultPath, usize> = BTreeMap::new();
    let before = lint::run(index, config);
    let mut applied = vec![];
    let mut current = index.clone();
    let mut diags = before.clone();
    for _ in 0..passes {
        let done = apply_pass(&current, &mut texts, &mut edits, &diags, opts)?;
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

    // A pass with any fix to apply applies one, so fixes are left only when the passes
    // ran out.
    let unapplied = diags.iter().filter(|d| opts.pick(d).is_some()).count();
    Ok(Plan { change, applied, remaining: diags, unapplied })
}

/// Applies one pass of fixes to `texts`, the texts of `index` (counting edits per file
/// in `edits`): every allowed fix of `diags` that neither overlaps nor touches one
/// taken before it. Frontmatter changes compose: each note's are applied as one
/// rewrite of its block, which only text edits can touch. A fix whose edits were all
/// taken already (another diagnostic's copy of it) is applied by them. Returns what it
/// applied.
fn apply_pass(
    index: &Index,
    texts: &mut BTreeMap<VaultPath, String>,
    edits: &mut BTreeMap<VaultPath, usize>,
    diags: &[Diagnostic],
    opts: &Options,
) -> Result<Vec<Applied>, Refusal> {
    let mut text_edits: BTreeMap<VaultPath, Vec<Edit>> = BTreeMap::new();
    let mut changes: BTreeMap<VaultPath, Vec<FmChange>> = BTreeMap::new();
    let touch = |a: &Range<usize>, b: &Range<usize>| a.start <= b.end && b.start <= a.end;
    let mut out = vec![];
    for (d, fix) in diags.iter().filter_map(|d| opts.pick(d).map(|f| (d, f))) {
        let planned = planned(index, fix)?;
        let copy = planned.iter().all(|(p, e)| match e {
            Planned::Text(e) => text_edits.get(p).is_some_and(|es| es.contains(e)),
            Planned::Frontmatter(c) => changes.get(p).is_some_and(|cs| cs.contains(c)),
        });
        if !copy {
            // Edits that touch one taken wait for the next pass.
            let clashes = planned.iter().any(|(p, e)| {
                let taken = text_edits.get(p).map_or(&[][..], Vec::as_slice);
                match e {
                    Planned::Text(e) => {
                        taken.iter().any(|t| touch(&t.range, &e.range))
                            || (changes.contains_key(p) && touch(&block(index, p), &e.range))
                    }
                    Planned::Frontmatter(_) => taken.iter().any(|t| touch(&t.range, &block(index, p))),
                }
            });
            if clashes {
                continue;
            }
            for (p, e) in planned {
                match e {
                    Planned::Text(e) => text_edits.entry(p).or_default().push(e),
                    Planned::Frontmatter(c) => changes.entry(p).or_default().push(c),
                }
            }
        }
        out.push(Applied { code: d.code.clone(), path: d.path().map(str::to_string), message: fix.message.clone() });
    }

    for (p, es) in &text_edits {
        *edits.entry(p.clone()).or_default() += es.len();
    }
    for (p, cs) in changes {
        *edits.entry(p.clone()).or_default() += cs.len();
        let note = index.parsed(&p).expect("`planned` checked the note");
        text_edits.entry(p).or_default().push(rewrite_frontmatter(note, &cs));
    }

    for (p, file_edits) in text_edits {
        let text = splice::apply(&texts[&p], file_edits).map_err(|e| Refusal::internal("lint.fix", e))?;
        texts.insert(p, text);
    }

    Ok(out)
}

/// A fix's edit, checked against the note it edits.
enum Planned {
    Text(Edit),
    Frontmatter(FmChange),
}

/// A fix's edits by vault path. Lint made them from `index`, so an edit naming another
/// file, a range outside its text, or a broken frontmatter is a loci bug (`lint.fix`).
fn planned(index: &Index, fix: &Fix) -> Result<Vec<(VaultPath, Planned)>, Refusal> {
    let bug = |m: String| Refusal::internal("lint.fix", m);
    let note = |path: &str| {
        let p = VaultPath::new(path.to_string()).ok_or_else(|| bug(format!("`{path}` is not a vault path")))?;
        let note = index.parsed(&p).ok_or_else(|| bug(format!("a fix edits `{p}`, which is not a note")))?;
        Ok::<_, Refusal>((p, note))
    };

    fix.edits
        .iter()
        .map(|e| match e {
            FixEdit::Text(e) => {
                let (p, n) = note(&e.path)?;
                let ok = e.byte_start <= e.byte_end
                    && e.byte_end <= n.text.len()
                    && n.text.is_char_boundary(e.byte_start)
                    && n.text.is_char_boundary(e.byte_end);
                if !ok {
                    return Err(bug(format!(
                        "a fix edits bytes {}..{} of `{p}`, outside its text",
                        e.byte_start, e.byte_end
                    )));
                }
                Ok((p, Planned::Text(Edit::replace(e.byte_start..e.byte_end, e.new.clone()))))
            }
            FixEdit::Frontmatter { path, frontmatter } => {
                let (p, n) = note(path)?;
                if n.frontmatter_broken() {
                    return Err(bug(format!("a fix changes the broken frontmatter of `{p}`")));
                }
                Ok((p, Planned::Frontmatter(frontmatter.clone())))
            }
        })
        .collect()
}

/// The bytes a rewrite of `path`'s frontmatter replaces: the block, or where one goes.
fn block(index: &Index, path: &VaultPath) -> Range<usize> {
    let note = index.parsed(path).expect("`planned` checked the note");
    // `planned` refused broken frontmatter, so a block here is closed.
    note.frontmatter.as_ref().map_or(note.body_start()..note.body_start(), |f| f.span.clone())
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
        let index = index::from_texts(&[
            ("concepts/a/x.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n---\n# X\n"),
            ("views/v.md", "# V\n\n[x](concepts/a/x.md)[x](concepts/a/x.md)\n"),
        ]);
        let config = Config::parse("schema = 1\n").unwrap();
        let opts = Options { unsafe_fixes: false, only: Some("link.markdown") };
        let one = plan_passes(&index, &config, &opts, 1).unwrap();
        assert_eq!((one.applied.len(), one.unapplied), (1, 1));
        let all = plan(&index, &config, &opts).unwrap();
        assert_eq!((all.applied.len(), all.unapplied), (2, 0));
    }
}
