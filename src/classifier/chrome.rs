use super::{outer_app_bundle, Classifier, Platform};

// Chrome/Chromium browsers. Self-identifying on every process via `--type=`.
// `proc_type`/`utility_type` are shared with Electron (which embeds Chromium).

pub(super) struct ChromiumClassifier;

impl Classifier for ChromiumClassifier {
    fn matches(&self, exe: &str, argv: &[&str]) -> bool {
        Self::is_exe(exe) || Self::brand_from_bundle(argv).is_some()
    }
    fn platform(&self) -> Platform {
        Platform::Chrome
    }
    fn label(&self, exe: &str, argv: &[&str]) -> String {
        let base = Self::brand(exe, argv);
        match Self::proc_type(argv) {
            Some(detail) => format!("{base} — {detail}"),
            // The browser/main process carries no --type= switch (absence is the signal).
            None => format!("{base} — browser"),
        }
    }
    fn groupable(&self) -> bool {
        true
    }
    fn group(&self, exe: &str, argv: &[&str]) -> Option<String> {
        Some(Self::brand(exe, argv).to_string())
    }
}

impl ChromiumClassifier {
    pub(super) fn is_exe(exe: &str) -> bool {
        matches!(
            exe,
            "chrome" | "chromium" | "chromium-browser" | "google-chrome" | "google-chrome-stable"
        )
    }

    /// The display brand for this process: "Chrome" or "Chromium". Linux exes
    /// are matched by basename; macOS processes are matched by their outermost
    /// `.app` bundle (helpers live nested inside the parent app — basename
    /// alone is something like "Google Chrome Helper (Renderer)").
    fn brand(exe: &str, argv: &[&str]) -> &'static str {
        match exe {
            "chromium" | "chromium-browser" => "Chromium",
            _ => Self::brand_from_bundle(argv).unwrap_or("Chrome"),
        }
    }

    /// macOS only: the brand inferred from the outermost `.app` bundle on the
    /// process's path. `None` on Linux (no bundle), or for an unfamiliar
    /// `.app`. Edge/Brave/etc. are deliberately not lumped into "Chrome" — a
    /// user wants to see the actual app they launched.
    fn brand_from_bundle(argv: &[&str]) -> Option<&'static str> {
        let bundle = outer_app_bundle(argv.first()?)?;
        match bundle.as_str() {
            "Google Chrome" | "Google Chrome Canary" | "Google Chrome Beta"
            | "Google Chrome Dev" => Some("Chrome"),
            "Chromium" => Some("Chromium"),
            _ => None,
        }
    }

    /// Maps a Chromium child's `--type=` (plus sub-flags) to a human label.
    /// `None` for the main/browser process (no `--type=`). Type values are
    /// authoritative per Chromium's `content_switches.cc` — see
    /// docs/chrome-processes.md. Reused by Electron and inheritance.
    pub(super) fn proc_type(argv: &[&str]) -> Option<String> {
        let proc_type = argv.iter().find_map(|p| p.strip_prefix("--type="))?;
        let detail = match proc_type {
            "renderer" => {
                if argv.contains(&"--extension-process") {
                    "renderer (extension)"
                } else {
                    "renderer"
                }
            }
            "gpu-process" => "GPU",
            "utility" => Self::utility_type(argv).unwrap_or("utility"),
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

    fn utility_type(argv: &[&str]) -> Option<&'static str> {
        let sub = argv
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