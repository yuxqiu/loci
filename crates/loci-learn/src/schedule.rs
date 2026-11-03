//! FSRS scheduling and estimates (learn-plugin §4).

use std::collections::BTreeMap;

use fsrs::{FSRS, FSRS6_DEFAULT_DECAY, FSRSItem, FSRSReview, MemoryState, current_retrievability};
use jiff::{Timestamp, ToSpan, tz::TimeZone};
use loci_core::id::Ulid;
use loci_core::parse::Spanned;

use crate::cards::Card;
use crate::reviewlog::{Grade, Line, Subject};

const SECONDS_PER_DAY: f64 = 86_400.0;

pub struct Scheduler {
    fsrs: FSRS,
    desired_retention: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardState {
    pub memory: MemoryState,
    pub last: Timestamp,
    pub due: Timestamp,
}

impl Scheduler {
    pub fn new(desired_retention: f32) -> Self {
        let fsrs = FSRS::new(&[]).expect("the default FSRS parameters are valid");
        Self { fsrs, desired_retention }
    }

    /// The state of a card from its lines, in timestamp order; `None` without lines.
    pub fn state(&self, lines: &[&Line]) -> Option<CardState> {
        let last = lines.last()?;
        let mut reviews = vec![];
        let mut prev: Option<Timestamp> = None;
        for l in lines {
            let delta_t = prev.map_or(0, |p| calendar_days(p, l.t));
            reviews.push(FSRSReview { rating: l.grade.rating(), delta_t });
            prev = Some(l.t);
        }
        let memory = self
            .fsrs
            .memory_state(FSRSItem { reviews }, None)
            .expect("ratings 1–4 and non-negative intervals give a valid memory state");
        let days = self.fsrs.next_interval(Some(memory.stability), self.desired_retention, last.grade.rating());
        let due = add_days(last.t, f64::from(days));
        Some(CardState { memory, last: last.t, due })
    }

    /// A concept's estimate and the lines it was computed from (§4.2).
    pub fn estimate<'a>(
        &self,
        note_cards: &[Spanned<Card>],
        concept: Ulid,
        lines: &'a [Line],
        now: Timestamp,
    ) -> (Estimate, Vec<&'a Line>) {
        let by_card = lines_by_card(lines);
        let mut rs = vec![];
        let mut used: Vec<&Line> = vec![];
        for c in note_cards {
            let Some(card_lines) = c.id.valid().and_then(|id| by_card.get(&id)) else { continue };
            if let Some(state) = self.state(card_lines) {
                rs.push(state.retrievability(now));
                used.extend(card_lines.iter().copied());
            }
        }

        if !rs.is_empty() {
            used.sort_by_key(|l| l.t);
            let r = rs.iter().sum::<f32>() / rs.len() as f32;
            return (Estimate::Cards { r }, used);
        }

        let items: Vec<&Line> = lines.iter().filter(|l| l.subject == Subject::Concept(concept)).collect();
        match items.last() {
            Some(last) => (Estimate::Item { grade: last.grade, age_days: days_since(last.t, now) }, items),
            None => (Estimate::Unknown, vec![]),
        }
    }

    /// Cards for a session (§2.6 step 3): due cards by due time, then new cards in the
    /// order of `all` ([`crate::cards::all`]'s order: by note path, then in note order).
    pub fn select<'a>(
        &self,
        all: &'a [Spanned<Card>],
        lines: &[Line],
        new_per_day: u32,
        now: Timestamp,
    ) -> (Vec<&'a Spanned<Card>>, usize) {
        let by_card = lines_by_card(lines);
        let mut due: Vec<(Timestamp, &Spanned<Card>)> = vec![];
        let mut fresh: Vec<&Spanned<Card>> = vec![];
        for c in all {
            let Some(id) = c.id.valid() else { continue };
            match by_card.get(&id).and_then(|ls| self.state(ls)) {
                Some(state) if state.due <= now => due.push((state.due, c)),
                Some(_) => {}
                None => fresh.push(c),
            }
        }

        due.sort_by_key(|(due, c)| (*due, c.id.valid()));
        let today = now.to_zoned(TimeZone::UTC).date();
        let introduced_today = by_card
            .values()
            .filter(|ls| ls.first().is_some_and(|l| l.t.to_zoned(TimeZone::UTC).date() == today))
            .count();
        let allowance = (new_per_day as usize).saturating_sub(introduced_today);
        let new_count = fresh.len().min(allowance);
        let mut out: Vec<&Spanned<Card>> = due.into_iter().map(|(_, c)| c).collect();
        out.extend(fresh.into_iter().take(new_count));
        (out, new_count)
    }
}

impl CardState {
    /// Retrievability of a card at `now` (§4.1).
    fn retrievability(&self, now: Timestamp) -> f32 {
        current_retrievability(self.memory, days_since(self.last, now) as f32, FSRS6_DEFAULT_DECAY)
    }
}

/// Days (fractional, not negative) from `then` to `now`.
fn days_since(then: Timestamp, now: Timestamp) -> f64 {
    (now.as_second() - then.as_second()).max(0) as f64 / SECONDS_PER_DAY
}

/// Whole days between the UTC calendar dates of `a` and `b` (§4.1).
fn calendar_days(a: Timestamp, b: Timestamp) -> u32 {
    let da = a.to_zoned(TimeZone::UTC).date();
    let db = b.to_zoned(TimeZone::UTC).date();
    let days = (db - da).get_days();
    u32::try_from(days.max(0)).expect("a non-negative day count fits in u32")
}

fn add_days(t: Timestamp, days: f64) -> Timestamp {
    let seconds = (days * SECONDS_PER_DAY).round() as i64;
    t.checked_add(seconds.seconds()).unwrap_or(Timestamp::MAX)
}

/// Groups card lines by card id, in timestamp order.
fn lines_by_card(lines: &[Line]) -> BTreeMap<Ulid, Vec<&Line>> {
    let mut out: BTreeMap<Ulid, Vec<&Line>> = BTreeMap::new();
    for l in lines {
        if let Subject::Card(c) = l.subject {
            out.entry(c).or_default().push(l);
        }
    }
    out
}

/// A concept's estimate (§4.2), serialized as §2.5 shows it (`{"kind": "cards", "r"}`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Estimate {
    Cards { r: f32 },
    Item { grade: Grade, age_days: f64 },
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::CardId;

    /// Test id `n`.
    fn id(n: u8) -> Ulid {
        format!("01ARZ3NDEKTSV4RRFFQ69G5F{n:02}").parse().unwrap()
    }

    fn line(t: &str, grade: Grade) -> Line {
        Line { t: t.parse().unwrap(), subject: Subject::Card(id(0)), grade }
    }

    #[test]
    fn calendar_day_counts() {
        let a: Timestamp = "2026-10-30T23:59:00Z".parse().unwrap();
        let b: Timestamp = "2026-10-31T00:01:00Z".parse().unwrap();
        assert_eq!(calendar_days(a, b), 1);
        assert_eq!(calendar_days(a, a), 0);
    }

    #[test]
    fn good_reviews_push_due_later() {
        let s = Scheduler::new(0.9);
        let l1 = line("2026-10-01T10:00:00Z", Grade::Good);
        let l2 = line("2026-10-04T10:00:00Z", Grade::Good);
        let one = s.state(&[&l1]).unwrap();
        let two = s.state(&[&l1, &l2]).unwrap();
        assert!(two.memory.stability > one.memory.stability);
        assert!(one.due > l1.t);
        // At the due time, retrievability is about the desired retention.
        let r = one.retrievability(one.due);
        assert!((r - 0.9).abs() < 0.02, "{r}");
    }

    #[test]
    fn again_is_due_sooner_than_easy() {
        let s = Scheduler::new(0.9);
        let a = s.state(&[&line("2026-10-01T10:00:00Z", Grade::Again)]).unwrap();
        let e = s.state(&[&line("2026-10-01T10:00:00Z", Grade::Easy)]).unwrap();
        assert!(a.due < e.due);
    }

    fn card(path: &str, n: u8) -> Spanned<Card> {
        let path = loci_core::path::VaultPath::new(path.to_string()).unwrap();
        Spanned::new(Card { path, id: CardId::Valid(id(n)), question: "q".into(), answer: "a".into() }, 0..0)
    }

    fn card_line(card: u8, t: &str, grade: Grade) -> Line {
        Line { t: t.parse().unwrap(), subject: Subject::Card(id(card)), grade }
    }

    #[test]
    fn selection_order_and_new_limit() {
        let all = vec![
            card("concepts/a/b.md", 3),
            card("concepts/a/b.md", 2),
            card("concepts/a/x.md", 4),
            card("concepts/a/z.md", 1),
        ];
        let lines = vec![
            card_line(4, "2026-01-01T00:00:00Z", Grade::Again),
            card_line(9, "2026-01-01T00:00:00Z", Grade::Good), // no such card
        ];
        let s = Scheduler::new(0.9);
        let (sel, n) = s.select(&all, &lines, 2, "2026-10-30T00:00:00Z".parse().unwrap());
        let ids: Vec<Ulid> = sel.iter().map(|c| c.id.valid().unwrap()).collect();
        assert_eq!(ids, [id(4), id(3), id(2)], "the due card, then new ones in card order");
        assert_eq!(n, 2);
    }

    #[test]
    fn estimates() {
        let s = Scheduler::new(0.9);
        let now: Timestamp = "2026-10-30T00:00:00Z".parse().unwrap();
        let cs = vec![card("concepts/a/x.md", 1), card("concepts/a/x.md", 2)];
        let lines = vec![card_line(1, "2026-10-29T00:00:00Z", Grade::Good)];
        let k = id(7);
        let (e, used) = s.estimate(&cs, k, &lines, now);
        assert!(matches!(e, Estimate::Cards { r } if r > 0.5 && r <= 1.0));
        assert_eq!(used.len(), 1);
        let quiz = vec![Line { subject: Subject::Concept(k), ..card_line(8, "2026-10-28T00:00:00Z", Grade::Hard) }];
        let (e, _) = s.estimate(&[], k, &quiz, now);
        assert!(matches!(e, Estimate::Item { grade: Grade::Hard, age_days } if (age_days - 2.0).abs() < 1e-9));
        assert_eq!(s.estimate(&[], k, &[], now).0, Estimate::Unknown);
    }
}
