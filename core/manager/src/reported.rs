//! Failures of work that repeats on a timer, such as the network reconciliation every 20
//! seconds. Each is logged at warn when it begins or its reason changes, and at info once it ends,
//! never again on every round while it stays the same.
use std::collections::HashMap;

/// The failure each subject last reported, by subject, such as `Docker reconciliation`.
#[derive(Default)]
pub(crate) struct Reported(HashMap<String, String>);

impl Reported {
    /// Report this round's outcome for `subject`, and say whether it was logged.
    pub(crate) fn outcome(&mut self, subject: &str, result: &anyhow::Result<()>) -> bool {
        match result {
            Ok(()) => {
                let ended = self.0.remove(subject).is_some();
                if ended {
                    tracing::info!("{subject} works again");
                }
                ended
            }
            Err(error) => {
                let error = format!("{error:#}");
                let changed = self.0.get(subject) != Some(&error);
                if changed {
                    tracing::warn!("{subject}: {error}");
                    self.0.insert(subject.to_owned(), error);
                }
                changed
            }
        }
    }
    /// Forget the subjects `keep` refuses, such as a deleted container's, without a line.
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.0.retain(|subject, _| keep(subject));
    }
}

#[cfg(test)]
#[path = "reported_tests.rs"]
mod tests;
