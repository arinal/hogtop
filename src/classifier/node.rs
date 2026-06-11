use super::{exe_basename, Classifier, Platform};

// Node, like Python, is invoked by name and so detectable as a runtime. One
// process, not groupable; the label stays the plain command.

pub(super) struct NodeClassifier;

impl Classifier for NodeClassifier {
    fn matches(&self, exe: &str, argv: &[&str]) -> bool {
        // npm/npx/node rewrite their process title to a single string (e.g.
        // "npm exec @playwright/mcp@latest"). sysinfo hands that back as one
        // argv[0] token, and the package's `/` makes the plain `exe` basename
        // garbage ("mcp@latest"). So basename the *leading word* of argv[0]
        // instead, falling back to the precomputed `exe` for the normal case.
        let head = argv
            .first()
            .and_then(|a| a.split_whitespace().next())
            .map(exe_basename)
            .unwrap_or_else(|| exe.to_string());
        matches!(head.as_str(), "node" | "nodejs" | "npm" | "npx")
    }
    fn platform(&self) -> Platform {
        Platform::Node
    }
    fn label(&self, _exe: &str, argv: &[&str]) -> String {
        // Same as Python: keep the default unclassified label (the full command).
        argv.join(" ")
    }
}