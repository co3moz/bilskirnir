//! Pins down how a script's end becomes its job's result: the exit code, whether it ran out of its
//! time, which `timeout` reports as 124, or 137 once the script outlived the TERM, only when the
//! time was up, and why a script that did not succeed failed.
use super::*;
use std::os::unix::process::ExitStatusExt;

fn ran(code: Option<i32>) -> Ran {
    Ran {
        status: code.map(|code| std::process::ExitStatus::from_raw(code << 8)),
        tail: "last line\n".into(),
    }
}

#[test]
fn a_script_reports_its_exit_code_and_whether_its_time_ran_out() {
    let succeeded = Outcome::of(&ran(Some(0)), false);
    assert_eq!(
        succeeded,
        Outcome {
            exit_code: Some(0),
            timed_out: false,
            output_tail: "last line\n".into(),
        }
    );
    assert_eq!(succeeded.failure(300), None);
    let failed = Outcome::of(&ran(Some(2)), false);
    assert!(!failed.timed_out);
    assert_eq!(failed.failure(300).unwrap(), "Script exited with code 2");
    for code in [124, 137] {
        let stopped = Outcome::of(&ran(Some(code)), true);
        assert!(stopped.timed_out, "{code}");
        assert_eq!(
            stopped.failure(30).unwrap(),
            format!(
                "Script ran out of its time limit of 30 seconds and was stopped (exit code {code})"
            )
        );
        // The script's own exit with that code, before its time, is no timeout.
        assert!(!Outcome::of(&ran(Some(code)), false).timed_out, "{code}");
    }
    // The manager's own limit killed it.
    let killed = Outcome::of(&ran(None), false);
    assert!(killed.timed_out && killed.exit_code.is_none());
}
