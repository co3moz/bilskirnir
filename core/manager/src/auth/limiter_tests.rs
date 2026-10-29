//! Pins down what the sign-in limit counts: failures and unanswered attempts per address for a
//! minute, never a success, and that a table full of other addresses neither refuses a new one
//! nor lifts a lockout while it can forget something else.
use super::*;

fn address(n: usize) -> Option<IpAddr> {
    Some(IpAddr::from([10, (n >> 16) as u8, (n >> 8) as u8, n as u8]))
}

fn fail(limiter: &Limiter, from: Option<IpAddr>, times: usize, now: Instant) {
    for _ in 0..times {
        assert_eq!(limiter.begin(from, now), Verdict::Counted);
    }
}

#[test]
fn an_address_is_refused_after_its_failures_and_another_is_not() {
    let (limiter, now) = (Limiter::default(), Instant::now());
    // Attempts that have not been answered yet count too, so a burst sent at once stops here.
    fail(&limiter, address(1), ATTEMPTS, now);
    assert_eq!(
        limiter.begin(address(1), now),
        Verdict::Refused { first: true }
    );
    assert_eq!(
        limiter.begin(address(1), now),
        Verdict::Refused { first: false }
    );
    assert_eq!(limiter.begin(address(2), now), Verdict::Counted);
    assert_eq!(limiter.begin(None, now), Verdict::Counted);
}

#[test]
fn successes_never_count_and_forget_the_failures_before_them() {
    let (limiter, now) = (Limiter::default(), Instant::now());
    for _ in 0..3 * ATTEMPTS {
        assert_eq!(limiter.begin(address(1), now), Verdict::Counted);
        limiter.succeeded(address(1));
    }
    fail(&limiter, address(1), ATTEMPTS - 1, now);
    assert_eq!(limiter.begin(address(1), now), Verdict::Counted);
    limiter.succeeded(address(1));
    fail(&limiter, address(1), ATTEMPTS, now);
    assert_eq!(
        limiter.begin(address(1), now),
        Verdict::Refused { first: true }
    );
}

#[test]
fn failures_count_for_a_minute() {
    let (limiter, now) = (Limiter::default(), Instant::now());
    fail(&limiter, address(1), ATTEMPTS / 2, now);
    let later = now + Duration::from_secs(30);
    fail(&limiter, address(1), ATTEMPTS / 2, later);
    assert!(matches!(
        limiter.begin(address(1), later),
        Verdict::Refused { .. }
    ));
    // The first half expires a minute after it began; the second still counts.
    let minute = now + WINDOW;
    fail(&limiter, address(1), ATTEMPTS / 2, minute);
    assert_eq!(
        limiter.begin(address(1), minute),
        Verdict::Refused { first: true }
    );
}

#[test]
fn a_full_table_takes_a_new_address_and_keeps_its_lockouts() {
    let (limiter, now) = (Limiter::default(), Instant::now());
    fail(&limiter, address(0), ATTEMPTS, now);
    for n in 1..4 * ADDRESSES {
        assert_eq!(limiter.begin(address(n), now), Verdict::Counted, "{n}");
        assert!(limiter.0.lock().unwrap().len() <= ADDRESSES);
    }
    assert!(matches!(
        limiter.begin(address(0), now),
        Verdict::Refused { .. }
    ));
    // Addresses whose failures expired go first, all of them at once.
    let later = now + WINDOW;
    assert_eq!(
        limiter.begin(address(4 * ADDRESSES), later),
        Verdict::Counted
    );
    assert_eq!(limiter.0.lock().unwrap().len(), 1);
}

#[test]
fn a_table_of_lockouts_forgets_the_oldest() {
    let (limiter, now) = (Limiter::default(), Instant::now());
    for n in 0..ADDRESSES {
        fail(
            &limiter,
            address(n),
            ATTEMPTS,
            now + Duration::from_millis(n as u64),
        );
    }
    let later = now + Duration::from_secs(1);
    assert_eq!(limiter.begin(address(ADDRESSES), later), Verdict::Counted);
    assert_eq!(limiter.begin(address(0), later), Verdict::Counted);
    assert!(matches!(
        limiter.begin(address(ADDRESSES - 1), later),
        Verdict::Refused { .. }
    ));
}
