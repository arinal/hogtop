use super::{Classifier, Platform};

// Node, like Python, is invoked by name and so detectable as a runtime. One
// process, not groupable; the label stays the plain command.

pub(super) struct NodeClassifier;

impl Classifier for NodeClassifier {
    fn matches(&self, exe: &str, _argv: &[&str]) -> bool {
        matches!(exe, "node" | "nodejs" | "npm")
    }
    fn platform(&self) -> Platform {
        Platform::Node
    }
    fn label(&self, _exe: &str, argv: &[&str]) -> String {
        // Same as Python: keep the default unclassified label (the full command).
        argv.join(" ")
    }
}