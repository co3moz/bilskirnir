//! Pins down that a repeating failure is logged when it begins, when its reason changes and when
//! it ends, and not on the rounds between.
use super::*;

fn failed(reason: &str) -> anyhow::Result<()> {
    Err(anyhow::anyhow!(reason.to_owned()))
}

#[test]
fn a_failure_is_logged_when_it_begins_changes_and_ends() {
    let mut reported = Reported::default();
    let subject = "Docker reconciliation";
    assert!(!reported.outcome(subject, &Ok(())));
    assert!(reported.outcome(subject, &failed("Missing manager label")));
    assert!(!reported.outcome(subject, &failed("Missing manager label")));
    assert!(reported.outcome(subject, &failed("Docker: no such container")));
    // Another subject's failure is its own.
    assert!(reported.outcome(
        "Container a network reconciliation",
        &failed("Docker: no such container")
    ));
    assert!(reported.outcome(subject, &Ok(())));
    assert!(!reported.outcome(subject, &Ok(())));
    assert!(reported.outcome(subject, &failed("Docker: no such container")));
}

#[test]
fn a_forgotten_subject_starts_again() {
    let mut reported = Reported::default();
    assert!(reported.outcome("Container a network reconciliation", &failed("gone")));
    assert!(reported.outcome("Container b network reconciliation", &failed("gone")));
    reported.retain(|subject| subject.contains(" b "));
    assert!(!reported.outcome("Container a network reconciliation", &Ok(())));
    assert!(!reported.outcome("Container b network reconciliation", &failed("gone")));
}
