use super::{Classifier, Platform};

// Same fan-out as Chromium, but its own convention: children carry `-contentproc`
// and the type as the *trailing* cmdline token, not a `--type=` switch.

pub(super) struct FirefoxClassifier;

impl Classifier for FirefoxClassifier {
    fn matches(&self, exe: &str, _argv: &[&str]) -> bool {
        matches!(exe, "firefox" | "firefox-bin" | "firefox-esr")
    }
    fn platform(&self) -> Platform {
        Platform::Firefox
    }
    fn label(&self, _exe: &str, argv: &[&str]) -> String {
        match Self::proc_type(argv) {
            Some(detail) => format!("Firefox — {detail}"),
            None => "Firefox".to_string(),
        }
    }
    fn groupable(&self) -> bool {
        true
    }
    fn group(&self, _exe: &str, _argv: &[&str]) -> Option<String> {
        Some("Firefox".to_string())
    }
}

impl FirefoxClassifier {
    fn proc_type(argv: &[&str]) -> Option<String> {
        if !argv.contains(&"-contentproc") {
            return None;
        }
        let detail = match *argv.last()? {
            "tab" => "tab",
            "gpu" => "GPU",
            "rdd" => "RDD", // Remote Data Decoder (audio/video)
            "socket" => "Socket service",
            "utility" => "Utility",
            "forkserver" => "fork server",
            "gmplugin" => "media plugin",
            other => other,
        };
        Some(detail.to_string())
    }
}