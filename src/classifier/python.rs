use super::{Classifier, Platform};

// Python is an interpreter invoked by name, so the process *is* detectable as a
// runtime (unlike a compiled binary, whose name says nothing). One process, not
// groupable — we only tag the platform and leave the label as the plain command.

pub(super) struct PythonClassifier;

impl Classifier for PythonClassifier {
    fn matches(&self, exe: &str, _argv: &[&str]) -> bool {
        // `python`, `python3`, `python3.12`, `pythonw`, plus the common tools.
        exe.starts_with("python") || exe == "pip" || exe == "ipython"
    }
    fn platform(&self) -> Platform {
        Platform::Python
    }
    fn label(&self, _exe: &str, argv: &[&str]) -> String {
        // Preserve the default (unclassified) label exactly — the whole command
        // line — so adding detection never changes how the process reads.
        argv.join(" ")
    }
}