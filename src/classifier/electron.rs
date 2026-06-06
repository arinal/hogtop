use std::path::Path;

use super::chrome::ChromiumClassifier;
use super::{Classifier, Platform};

// Chromium under the hood, so it reuses Chromium's `--type=` taxonomy. The app
// identity comes from the runtime exe or the `app.asar` path; generic
// shared-runtime children have none and inherit it from an ancestor.

pub(super) struct ElectronClassifier;

impl Classifier for ElectronClassifier {
    fn matches(&self, exe: &str, parts: &[&str]) -> bool {
        Self::is_electron(exe, parts)
    }
    fn platform(&self) -> Platform {
        Platform::Electron
    }
    fn label(&self, exe: &str, parts: &[&str]) -> String {
        let app_name = Self::app(exe, parts).unwrap_or_else(|| exe.to_string());
        match ChromiumClassifier::proc_type(parts) {
            Some(detail) => format!("{app_name} — {detail}"),
            None => app_name,
        }
    }
    fn groupable(&self) -> bool {
        true
    }
    fn group(&self, exe: &str, parts: &[&str]) -> Option<String> {
        // `None` for generic shared-runtime children → they inherit upstream.
        Self::app(exe, parts)
    }
}

impl ElectronClassifier {
    fn is_electron(exe: &str, parts: &[&str]) -> bool {
        const INDICATORS: [&str; 3] = ["electron", "--ms-enable-electron", "--type="];
        const KNOWN: [&str; 9] = [
            "code", "codium", "slack", "discord", "signal-desktop", "obsidian", "spotify",
            "teams", "cursor",
        ];
        if KNOWN.contains(&exe) {
            return true;
        }
        parts
            .iter()
            .any(|p| INDICATORS.iter().any(|ind| p.contains(ind)))
            && !ChromiumClassifier::is_exe(exe)
    }

    /// The app name from a known runtime exe or an `app.asar` path. `None` for
    /// generic children (no identity of their own — they inherit).
    fn app(exe: &str, parts: &[&str]) -> Option<String> {
        let known = match exe {
            "code" | "codium" => Some("VS Code"),
            "slack" => Some("Slack"),
            "discord" | "Discord" => Some("Discord"),
            "signal-desktop" => Some("Signal"),
            "obsidian" => Some("Obsidian"),
            "spotify" => Some("Spotify"),
            "teams" => Some("Teams"),
            "cursor" => Some("Cursor"),
            _ => None,
        };
        // Shared-runtime apps (e.g. Arch's electron37) launch as `.../electron
        // <app.asar>`; derive the name from the .asar path.
        known.map(str::to_string).or_else(|| Self::app_from_asar(parts))
    }

    /// `/usr/lib/bitwarden/app.asar` → "Bitwarden". The app dir is the asar's
    /// parent, skipping a `resources/` wrapper (`/opt/Foo/resources/app.asar` → "Foo").
    fn app_from_asar(parts: &[&str]) -> Option<String> {
        let asar = parts.iter().find(|p| p.ends_with(".asar"))?;
        let mut dir = Path::new(asar).parent()?;
        if dir.file_name().and_then(|s| s.to_str()) == Some("resources") {
            dir = dir.parent().unwrap_or(dir);
        }
        let name = dir.file_name()?.to_str()?;
        let mut chars = name.chars();
        let first = chars.next()?;
        Some(first.to_uppercase().collect::<String>() + chars.as_str())
    }
}