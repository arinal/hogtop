use super::{Classifier, Platform};

// Chrome/Chromium browsers. Self-identifying on every process via `--type=`.
// `proc_type`/`utility_type` are shared with Electron (which embeds Chromium).

pub(super) struct ChromiumClassifier;

impl Classifier for ChromiumClassifier {
    fn matches(&self, exe: &str, _parts: &[&str]) -> bool {
        Self::is_exe(exe)
    }
    fn platform(&self) -> Platform {
        Platform::Chrome
    }
    fn label(&self, exe: &str, parts: &[&str]) -> String {
        let base = match exe {
            "chromium" | "chromium-browser" => "Chromium",
            _ => "Chrome",
        };
        match Self::proc_type(parts) {
            Some(detail) => format!("{base} — {detail}"),
            // The browser/main process carries no --type= switch (absence is the signal).
            None => format!("{base} — browser"),
        }
    }
    fn groupable(&self) -> bool {
        true
    }
    fn group(&self, exe: &str, _parts: &[&str]) -> Option<String> {
        Some(
            match exe {
                "chromium" | "chromium-browser" => "Chromium",
                _ => "Chrome",
            }
            .to_string(),
        )
    }
}

impl ChromiumClassifier {
    pub(super) fn is_exe(exe: &str) -> bool {
        matches!(
            exe,
            "chrome" | "chromium" | "chromium-browser" | "google-chrome" | "google-chrome-stable"
        )
    }

    /// Maps a Chromium child's `--type=` (plus sub-flags) to a human label.
    /// `None` for the main/browser process (no `--type=`). Type values are
    /// authoritative per Chromium's `content_switches.cc` — see
    /// docs/chrome-processes.md. Reused by Electron and inheritance.
    pub(super) fn proc_type(parts: &[&str]) -> Option<String> {
        let proc_type = parts.iter().find_map(|p| p.strip_prefix("--type="))?;
        let detail = match proc_type {
            "renderer" => {
                if parts.contains(&"--extension-process") {
                    "renderer (extension)"
                } else {
                    "renderer"
                }
            }
            "gpu-process" => "GPU",
            "utility" => Self::utility_type(parts).unwrap_or("utility"),
            "zygote" => "zygote",
            "sandbox-ipc" => "sandbox IPC", // Linux-only helper
            // Not content/ process types, but seen in the wild: crashpad-handler
            // comes from the Crashpad component, broker is the Windows sandbox.
            "crashpad-handler" => "crashpad",
            "broker" => "broker",
            other => return Some(other.to_string()),
        };
        Some(detail.to_string())
    }

    fn utility_type(parts: &[&str]) -> Option<&'static str> {
        let sub = parts
            .iter()
            .find_map(|p| p.strip_prefix("--utility-sub-type="))?;
        Some(if sub.contains("NetworkService") {
            "Network service"
        } else if sub.contains("StorageService") {
            "Storage service"
        } else if sub.contains("AudioService") {
            "Audio service"
        } else if sub.contains("DataDecoderService") {
            "Data Decoder service"
        } else if sub.contains("VideoCapture") {
            "Video capture"
        } else {
            return None;
        })
    }
}