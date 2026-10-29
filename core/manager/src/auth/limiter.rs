//! Failed sign-ins, counted by the address they come from.
//!
//! An attempt counts from the moment its password check begins, so a burst sent at once stops at
//! the limit instead of being checked in full before its first failure is known, and a success
//! forgets its address's failures, so signing in never uses up the attempts. The table keeps at most [`ADDRESSES`] addresses. A full one forgets
//! an address rather than refusing a new one: a flood from many addresses costs a bounded table
//! and never locks the administrator out.
use std::{
    collections::{HashMap, VecDeque},
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Failed sign-ins one address may make in [`WINDOW`].
pub(crate) const ATTEMPTS: usize = 20;
/// How long a failed sign-in counts.
const WINDOW: Duration = Duration::from_secs(60);
/// Addresses whose failures are kept.
const ADDRESSES: usize = 1024;

/// The failed sign-ins of the last minute, by the address they came from. Requests without a
/// recorded address, as the tests make, share `None`.
#[derive(Default)]
pub struct Limiter(Mutex<HashMap<Option<IpAddr>, Failures>>);

#[derive(Default)]
struct Failures {
    /// When each failed or still unanswered attempt began, oldest first. Never more than
    /// [`ATTEMPTS`], since a locked address's attempts are refused before they count.
    times: VecDeque<Instant>,
    /// Whether the lockout that holds now was reported.
    reported: bool,
}

impl Failures {
    fn expire(&mut self, now: Instant) {
        while self.times.front().is_some_and(|at| expired(*at, now)) {
            self.times.pop_front();
        }
    }
    fn locked(&self) -> bool {
        self.times.len() >= ATTEMPTS
    }
}

fn expired(at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(at) >= WINDOW
}

/// What [`Limiter::begin`] decided about an attempt.
#[derive(Debug, PartialEq)]
pub(crate) enum Verdict {
    /// The attempt may check its password, and counts as a failure unless it succeeds.
    Counted,
    /// The address used its attempts. `first` for the first refusal of this lockout, the one
    /// worth a log line, since a refusal costs the sender nothing and may come in thousands.
    Refused { first: bool },
}

impl Limiter {
    /// Count an attempt from `from` before its password is checked, or refuse it.
    pub(crate) fn begin(&self, from: Option<IpAddr>, now: Instant) -> Verdict {
        let mut table = self.0.lock().unwrap();
        if table.len() >= ADDRESSES && !table.contains_key(&from) {
            make_room(&mut table, now);
        }
        let failures = table.entry(from).or_default();
        failures.expire(now);
        if failures.locked() {
            let first = !std::mem::replace(&mut failures.reported, true);
            return Verdict::Refused { first };
        }
        failures.reported = false;
        failures.times.push_back(now);
        Verdict::Counted
    }
    /// A sign-in from `from` succeeded: its address starts again with no failures.
    pub(crate) fn succeeded(&self, from: Option<IpAddr>) {
        self.0.lock().unwrap().remove(&from);
    }
}

/// Forget the addresses whose failures all expired, and when that frees nothing, the one that
/// failed least recently, preferring an address still under its limit, so a flood of new
/// addresses lifts no lockout while there is anything else to forget.
fn make_room(table: &mut HashMap<Option<IpAddr>, Failures>, now: Instant) {
    table.retain(|_, failures| failures.times.back().is_some_and(|at| !expired(*at, now)));
    if table.len() < ADDRESSES {
        return;
    }
    let forgotten = table
        .iter()
        .min_by_key(|(_, failures)| (failures.locked(), failures.times.back().copied()))
        .map(|(from, _)| *from);
    if let Some(from) = forgotten {
        table.remove(&from);
    }
}

#[cfg(test)]
#[path = "limiter_tests.rs"]
mod tests;
