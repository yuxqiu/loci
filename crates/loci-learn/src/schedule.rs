//! FSRS scheduling and estimates (learn-plugin §4).

use std::collections::BTreeMap;

use fsrs::{FSRS, FSRS6_DEFAULT_DECAY, FSRSItem, FSRSReview, MemoryState, current_retrievability};
use jiff::{Timestamp, ToSpan, tz::TimeZone};

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
}

/// Retrievability of a card at `now` (§4.1).
fn retrievability(state: &CardState, now: Timestamp) -> f32 {
    let elapsed = (now.as_second() - state.last.as_second()).max(0) as f64 / SECONDS_PER_DAY;
    current_retrievability(state.memory, elapsed as f32, FSRS6_DEFAULT_DECAY)
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
fn lines_by_card(lines: &[Line]) -> BTreeMap<&str, Vec<&Line>> {
    let mut out: BTreeMap<&str, Vec<&Line>> = BTreeMap::new();
    for l in lines {
        if let Subject::Card(c) = &l.subject {
            out.entry(c.as_str()).or_default().push(l);
        }
    }
    out
}

/// A concept's estimate (§4.2).
#[derive(Debug, Clone, PartialEq)]
pub enum Estimate {
    Cards { r: f32 },
    Item { grade: Grade, age_days: f64 },
    Unknown,
}

/// A concept's estimate and the lines it was computed from (§4.2).
pub fn estimate<'a>(
    note_cards: &[Card],
    concept: &str,
    lines: &'a [Line],
    scheduler: &Scheduler,
    now: Timestamp,
) -> (Estimate, Vec<&'a Line>) {
    let by_card = lines_by_card(lines);
    let mut rs = vec![];
    let mut used: Vec<&Line> = vec![];
    for c in note_cards {
        let Some(card_lines) = c.id.as_deref().and_then(|id| by_card.get(id)) else { continue };
        if let Some(state) = scheduler.state(card_lines) {
            rs.push(retrievability(&state, now));
            used.extend(card_lines.iter().copied());
        }
    }
    if !rs.is_empty() {
        used.sort_by_key(|l| l.t);
        let r = rs.iter().sum::<f32>() / rs.len() as f32;
        return (Estimate::Cards { r }, used);
    }
    let items: Vec<&Line> =
        lines.iter().filter(|l| matches!(&l.subject, Subject::Concept(c) if c == concept)).collect();
    match items.last() {
        Some(last) => {
            let age_days = (now.as_second() - last.t.as_second()).max(0) as f64 / SECONDS_PER_DAY;
            (Estimate::Item { grade: last.grade, age_days }, items)
        }
        None => (Estimate::Unknown, vec![]),
    }
}

/// Cards for a session (§2.6 step 3): due cards by due time, then new cards.
pub fn select<'a>(
    all: &'a [Card],
    lines: &[Line],
    scheduler: &Scheduler,
    new_per_day: u32,
    now: Timestamp,
) -> (Vec<&'a Card>, usize) {
    let by_card = lines_by_card(lines);
    let mut due: Vec<(Timestamp, &Card)> = vec![];
    let mut fresh: Vec<&Card> = vec![];
    for c in all {
        let Some(id) = c.id.as_deref() else { continue };
        match by_card.get(id).and_then(|ls| scheduler.state(ls)) {
            Some(state) if state.due <= now => due.push((state.due, c)),
            Some(_) => {}
            None => fresh.push(c),
        }
    }
    due.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
    fresh.sort_by(|a, b| a.path.cmp(&b.path).then(a.position.cmp(&b.position)));
    let today = now.to_zoned(TimeZone::UTC).date();
    let introduced_today =
        by_card.values().filter(|ls| ls.first().is_some_and(|l| l.t.to_zoned(TimeZone::UTC).date() == today)).count();
    let allowance = (new_per_day as usize).saturating_sub(introduced_today);
    let new_count = fresh.len().min(allowance);
    let mut out: Vec<&Card> = due.into_iter().map(|(_, c)| c).collect();
    out.extend(fresh.into_iter().take(new_count));
    (out, new_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(t: &str, grade: Grade) -> Line {
        Line { t: t.parse().unwrap(), subject: Subject::Card("C".into()), grade }
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
        let r = retrievability(&one, one.due);
        assert!((r - 0.9).abs() < 0.02, "{r}");
    }

    #[test]
    fn again_is_due_sooner_than_easy() {
        let s = Scheduler::new(0.9);
        let a = s.state(&[&line("2026-10-01T10:00:00Z", Grade::Again)]).unwrap();
        let e = s.state(&[&line("2026-10-01T10:00:00Z", Grade::Easy)]).unwrap();
        assert!(a.due < e.due);
    }

    fn card(path: &str, pos: usize, id: &str) -> Card {
        Card {
            path: path.to_string().try_into().unwrap(),
            range: 0..0,
            position: pos,
            id: Some(id.into()),
            question: "q".into(),
            answer: "a".into(),
        }
    }

    fn card_line(card: &str, t: &str, grade: Grade) -> Line {
        Line { t: t.parse().unwrap(), subject: Subject::Card(card.into()), grade }
    }

    #[test]
    fn selection_order_and_new_limit() {
        let all = vec![
            card("concepts/a/z.md", 0, "N1"),
            card("concepts/a/b.md", 1, "N2"),
            card("concepts/a/b.md", 0, "N3"),
            card("concepts/a/x.md", 0, "D1"),
        ];
        let lines = vec![
            card_line("D1", "2026-01-01T00:00:00Z", Grade::Again),
            card_line("GONE", "2026-01-01T00:00:00Z", Grade::Good),
        ];
        let s = Scheduler::new(0.9);
        let (sel, n) = select(&all, &lines, &s, 2, "2026-10-30T00:00:00Z".parse().unwrap());
        let ids: Vec<&str> = sel.iter().map(|c| c.id.as_deref().unwrap()).collect();
        assert_eq!(ids, vec!["D1", "N3", "N2"]);
        assert_eq!(n, 2);
    }

    #[test]
    fn estimates() {
        let s = Scheduler::new(0.9);
        let now: Timestamp = "2026-10-30T00:00:00Z".parse().unwrap();
        let cs = vec![card("concepts/a/x.md", 0, "C1"), card("concepts/a/x.md", 1, "C2")];
        let lines = vec![card_line("C1", "2026-10-29T00:00:00Z", Grade::Good)];
        let (e, used) = estimate(&cs, "K", &lines, &s, now);
        assert!(matches!(e, Estimate::Cards { r } if r > 0.5 && r <= 1.0));
        assert_eq!(used.len(), 1);
        let quiz =
            vec![Line { subject: Subject::Concept("K".into()), ..card_line("x", "2026-10-28T00:00:00Z", Grade::Hard) }];
        let (e, _) = estimate(&[], "K", &quiz, &s, now);
        assert!(matches!(e, Estimate::Item { grade: Grade::Hard, age_days } if (age_days - 2.0).abs() < 1e-9));
        assert_eq!(estimate(&[], "K", &[], &s, now).0, Estimate::Unknown);
    }
}
