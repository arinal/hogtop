use super::{Classifier, Platform};

// A shell is an interpreter invoked by name, so — like Python or Node — the
// process *is* detectable as a runtime. This catches both interactive shells
// and scripts (`/bin/sh -c …`). One process, not groupable; the label stays the
// plain command. Login shells arrive as `-bash`/`-zsh` (a leading dash), so the
// dash is stripped before matching.

pub(super) struct ShellClassifier;

/// The common Unix shells, by exe basename (dash already stripped).
const SHELLS: [&str; 9] = ["sh", "bash", "zsh", "fish", "dash", "ksh", "tcsh", "csh", "ash"];

impl Classifier for ShellClassifier {
    fn matches(&self, exe: &str, _argv: &[&str]) -> bool {
        // Login shells prefix argv[0] with `-` (e.g. `-bash`); strip it first.
        let name = exe.strip_prefix('-').unwrap_or(exe);
        SHELLS.contains(&name)
    }
    fn platform(&self) -> Platform {
        Platform::Shell
    }
    fn label(&self, _exe: &str, argv: &[&str]) -> String {
        // Keep the default unclassified label (the full command line).
        argv.join(" ")
    }
}
