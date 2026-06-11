use std::path::Path;

use super::apps::{app_by_bundle, app_by_exe};
use super::chrome::ChromiumClassifier;
use super::{outer_app_bundle, Classifier, Platform};

// Chromium under the hood, so it reuses Chromium's `--type=` taxonomy. The app
// identity comes from the runtime exe, the `app.asar` path, or — on macOS —
// the outermost `.app` bundle on the path. Generic shared-runtime children
// have no identity of their own and inherit it from an ancestor.

pub(super) struct ElectronClassifier;

impl Classifier for ElectronClassifier {
    fn matches(&self, exe: &str, argv: &[&str]) -> bool {
        Self::is_electron(exe, argv)
    }
    fn platform(&self) -> Platform {
        Platform::Electron
    }
    fn label(&self, exe: &str, argv: &[&str]) -> String {
        let app_name = Self::app(exe, argv).unwrap_or_else(|| exe.to_string());
        match ChromiumClassifier::proc_type(argv) {
            Some(detail) => format!("{app_name} — {detail}"),
            None => app_name,
        }
    }
    fn groupable(&self) -> bool {
        true
    }
    fn group(&self, exe: &str, argv: &[&str]) -> Option<String> {
        // `None` for generic shared-runtime children → they inherit upstream.
        Self::app(exe, argv)
    }
}

impl ElectronClassifier {
    fn is_electron(exe: &str, argv: &[&str]) -> bool {
        const INDICATORS: [&str; 3] = ["electron", "--ms-enable-electron", "--type="];
        // A known Linux exe basename is conclusive on its own.
        if app_by_exe(exe).is_some() {
            return true;
        }
        // macOS: a known `.app` bundle on the path is likewise conclusive.
        if let Some(first) = argv.first()
            && let Some(bundle) = outer_app_bundle(first)
            && app_by_bundle(&bundle).is_some()
        {
            return true;
        }
        argv.iter()
            .any(|p| INDICATORS.iter().any(|ind| p.contains(ind)))
            && !ChromiumClassifier::is_exe(exe)
    }

    /// The app name from a known runtime exe, an `app.asar` path, or — on
    /// macOS — the outermost `.app` bundle. `None` for generic children with
    /// no identity of their own (they inherit). Known exes/bundles resolve via
    /// the central app registry; the `.asar` path is structural and handled here.
    fn app(exe: &str, argv: &[&str]) -> Option<String> {
        if let Some(id) = app_by_exe(exe) {
            return Some(id.name().to_string());
        }
        if let Some(first) = argv.first()
            && let Some(bundle) = outer_app_bundle(first)
            && let Some(id) = app_by_bundle(&bundle)
        {
            return Some(id.name().to_string());
        }
        Self::app_from_asar(argv)
    }

    /// Shared-runtime apps (e.g. Arch's electron37) launch as
    /// `.../electron <app.asar>`; derive the name from the .asar path.
    /// `/usr/lib/bitwarden/app.asar` → "Bitwarden". The app dir is the asar's
    /// parent, skipping a `resources/` wrapper (`/opt/Foo/resources/app.asar` → "Foo").
    fn app_from_asar(argv: &[&str]) -> Option<String> {
        let asar = argv.iter().find(|p| p.ends_with(".asar"))?;
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