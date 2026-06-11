use super::{Classifier, Platform};

// Kernel threads have no readable command line, so the sampler surfaces them by
// their bracketed comm name — `[kthreadd]`, `[kworker/0:1]`, `[rcu_sched]`. That
// bracketing is the signal: a process whose argv[0] is `[…]` is kernel-owned.
// One process, not groupable; the label stays the bracketed name. Matching reads
// argv[0] directly, not the exe basename, because names like `[kworker/0:1]`
// contain a `/` that would wreck a path-basename split.

pub(super) struct KernelClassifier;

impl Classifier for KernelClassifier {
    fn matches(&self, _exe: &str, argv: &[&str]) -> bool {
        argv.first()
            .is_some_and(|a| a.starts_with('[') && a.ends_with(']'))
    }
    fn platform(&self) -> Platform {
        Platform::Kernel
    }
    fn label(&self, _exe: &str, argv: &[&str]) -> String {
        // Keep the default unclassified label (the bracketed comm name).
        argv.join(" ")
    }
}
