//! FSRS scheduling and estimates (learn-plugin §4).

use std::collections::BTreeMap;

use fsrs::{FSRS, FSRS6_DEFAULT_DECAY, FSRSItem, FSRSReview, MemoryState, current_retrievability};
use jiff::{Timestamp, ToSpan, tz::TimeZone};

use crate::reviewlog::{Grade, Line};

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
pub fn retrievability(state: &CardState, now: Timestamp) -> f32 {
    let elapsed = (now.as_second() - state.last.as_second()).max(0) as f64 / SECONDS_PER_DAY;
    current_retrievability(state.memory, elapsed as f32, FSRS6_DEFAULT_DECAY)
}

/// Whole days between the UTC calendar dates of `a` and `b` (§4.1).
pub fn calendar_days(a: Timestamp, b: Timestamp) -> u32 {
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
pub fn lines_by_card(lines: &[Line]) -> BTreeMap<&str, Vec<&Line>> {
    let mut out: BTreeMap<&str, Vec<&Line>> = BTreeMap::new();
    for l in lines {
        if let Some(c) = &l.card {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reviewlog::{Grader, Purpose};

    fn line(t: &str, grade: Grade) -> Line {
        Line {
            t: t.parse().unwrap(),
            card: Some("C".into()),
            concept: None,
            item: None,
            purpose: Purpose::Review,
            grader: Grader::Myself,
            grade,
            verdicts: None,
        }
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
}
