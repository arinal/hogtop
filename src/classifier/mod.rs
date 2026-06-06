use std::path::Path;

mod chrome;
mod electron;
mod firefox;
mod java;

use chrome::ChromiumClassifier;
use electron::ElectronClassifier;
use firefox::FirefoxClassifier;
use java::JavaClassifier;

/// The runtime family a process belongs to — its intrinsic "platform", detected
/// by the classifier and carried as a plain process attribute (alongside pid,
/// memory, etc.). Consumers use it for presentation (icon selection) without
/// re-running detection. `Other` is anything no family recognises.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Chrome,
    Firefox,
    Electron,
    Java,
    Other,
}

/// The [`Platform`] this command line belongs to, or [`Platform::Other`] if no
/// family recognises it.
pub fn platform(raw_cmd: &str) -> Platform {
    let parts: Vec<&str> = raw_cmd.split_whitespace().collect();
    let Some(first) = parts.first() else {
        return Platform::Other;
    };
    let exe = exe_basename(first);
    family_for(&exe, &parts).map_or(Platform::Other, |f| f.platform())
}

/// Turn a raw process command line into a plain display name (process-kind, no
/// icon). The icon is a presentation concern added by the UI at render time.
pub fn friendly_name(raw_cmd: &str) -> String {
    let parts: Vec<&str> = raw_cmd.split_whitespace().collect();
    let Some(first) = parts.first() else {
        return raw_cmd.to_string();
    };
    let exe = exe_basename(first);
    match family_for(&exe, &parts) {
        Some(f) => f.label(&exe, &parts),
        None => raw_cmd.to_string(),
    }
}

/// The group identity this cmdline yields *on its own* (Chrome/Chromium,
/// Firefox, or an Electron app). `None` for generic Electron children — which
/// carry no identity and inherit it from an ancestor — and for non-groupable or
/// unrecognised processes. Self-identifying families (Chromium, Firefox) yield
/// a name on *every* process, so they never need inheritance.
pub fn group_app(raw_cmd: &str) -> Option<String> {
    let parts: Vec<&str> = raw_cmd.split_whitespace().collect();
    let exe = exe_basename(parts.first()?);
    match family_for(&exe, &parts) {
        Some(f) if f.groupable() => f.group(&exe, &parts),
        _ => None,
    }
}

/// Whether the cmdline belongs to a groupable multi-process app family. A
/// groupable process with no `group_app` of its own is a generic Electron child
/// that inherits its name from an ancestor (a shared-runtime zygote).
pub fn is_groupable_family(raw_cmd: &str) -> bool {
    let parts: Vec<&str> = raw_cmd.split_whitespace().collect();
    let Some(first) = parts.first() else {
        return false;
    };
    let exe = exe_basename(first);
    family_for(&exe, &parts).is_some_and(|f| f.groupable())
}

/// Compose a label for a process that inherits `app` from an ancestor, using
/// this process's own `--type` detail (e.g. inherited "Bitwarden" + own
/// "zygote" → "Bitwarden — zygote").
pub fn inherited_label(app: &str, raw_cmd: &str) -> String {
    let parts: Vec<&str> = raw_cmd.split_whitespace().collect();
    // Generic children are Electron's case, and Electron uses Chromium's --type=.
    match ChromiumClassifier::proc_type(&parts) {
        Some(detail) => format!("{app} — {detail}"),
        None => app.to_string(),
    }
}

/// A process *family* — a class of programs recognised and labelled the same
/// way (Java, Chromium, Firefox, Electron). Each family owns its detection,
/// its display label, and — if it fans out into multiple OS processes — its
/// app-group identity. Adding a new multi-process app is one more impl
/// registered in [`FAMILIES`]; nothing else changes.
pub(crate) trait Classifier {
    /// Does this command line belong to this family?
    fn matches(&self, exe: &str, parts: &[&str]) -> bool;

    /// Human display label for this process (e.g. "Chrome — renderer").
    fn label(&self, exe: &str, parts: &[&str]) -> String;

    /// The runtime family this classifier represents.
    fn platform(&self) -> Platform;

    /// Whether this family's processes roll up into a single app row.
    fn groupable(&self) -> bool {
        false
    }

    /// App-group identity derivable from this command line alone, when
    /// [`groupable`](Self::groupable). `None` means "in the family but
    /// anonymous" — a generic child that inherits its name from an ancestor
    /// (only shared-runtime Electron children hit this).
    fn group(&self, _exe: &str, _parts: &[&str]) -> Option<String> {
        None
    }
}

/// Families tried in order. Java is first so a JVM process whose args happen to
/// contain Chromium-ish flags still reads as Java.
const FAMILIES: &[&dyn Classifier] = &[
    &JavaClassifier,
    &ChromiumClassifier,
    &FirefoxClassifier,
    &ElectronClassifier,
];

fn family_for(exe: &str, parts: &[&str]) -> Option<&'static dyn Classifier> {
    FAMILIES.iter().copied().find(|f| f.matches(exe, parts))
}

pub(crate) fn exe_basename(arg: &str) -> String {
    Path::new(arg)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| arg.to_string())
}

#[cfg(test)]
mod tests {
    use super::friendly_name;

    // Representative cmdlines captured from a live browser. Renderer/helper
    // cmdlines are space-joined (argv rewritten post-launch); friendly_name
    // splits on whitespace, so both NUL- and space-joined inputs work.
    const CHROME: &str = "/opt/google/chrome/chrome";

    #[test]
    fn chrome_process_types() {
        assert_eq!(
            friendly_name(&format!("{CHROME} --type=renderer --renderer-client-id=44")),
            "Chrome — renderer"
        );
        assert_eq!(
            friendly_name(&format!(
                "{CHROME} --type=renderer --extension-process --renderer-client-id=7"
            )),
            "Chrome — renderer (extension)"
        );
        assert_eq!(
            friendly_name(&format!("{CHROME} --type=gpu-process --ozone-platform=wayland")),
            "Chrome — GPU"
        );
        assert_eq!(
            friendly_name(&format!("{CHROME} --type=zygote")),
            "Chrome — zygote"
        );
        assert_eq!(
            friendly_name(&format!("{CHROME} --type=sandbox-ipc")),
            "Chrome — sandbox IPC"
        );
        // Browser/main process: no --type= switch.
        assert_eq!(friendly_name(CHROME), "Chrome — browser");
    }

    #[test]
    fn chrome_utility_subtypes() {
        let case = |svc: &str| {
            friendly_name(&format!(
                "{CHROME} --type=utility --utility-sub-type={svc} --lang=en-US"
            ))
        };
        assert_eq!(case("network.mojom.NetworkService"), "Chrome — Network service");
        assert_eq!(case("storage.mojom.StorageService"), "Chrome — Storage service");
        assert_eq!(case("audio.mojom.AudioService"), "Chrome — Audio service");
        assert_eq!(
            case("data_decoder.mojom.DataDecoderService"),
            "Chrome — Data Decoder service"
        );
        // Unknown sub-type falls back to the bare "utility".
        assert_eq!(case("some.mojom.MysteryService"), "Chrome — utility");
    }

    #[test]
    fn chromium_branding() {
        assert_eq!(
            friendly_name("/usr/bin/chromium --type=gpu-process"),
            "Chromium — GPU"
        );
    }

    #[test]
    fn firefox_dialect() {
        // Main process: no -contentproc.
        assert_eq!(friendly_name("/usr/lib/firefox/firefox"), "Firefox");
        // Children carry the type as the trailing token.
        assert_eq!(
            friendly_name("/usr/lib/firefox/firefox -contentproc -parentPid 100 7 tab"),
            "Firefox — tab"
        );
        assert_eq!(
            friendly_name("/usr/lib/firefox/firefox -contentproc -parentPid 100 4 rdd"),
            "Firefox — RDD"
        );
        assert_eq!(
            friendly_name("/usr/lib/firefox/firefox -contentproc -parentPid 100 1 forkserver"),
            "Firefox — fork server"
        );
    }

    #[test]
    fn electron_apps_reuse_taxonomy() {
        assert_eq!(
            friendly_name("/usr/share/code/code --type=renderer"),
            "VS Code — renderer"
        );
        assert_eq!(friendly_name("/usr/bin/slack --type=gpu-process"), "Slack — GPU");
        // Electron main process → just the app name.
        assert_eq!(friendly_name("/usr/share/code/code"), "VS Code");
    }

    #[test]
    fn electron_shared_runtime_named_from_asar() {
        // Shared electron runtime: app identity comes from the .asar path.
        assert_eq!(
            friendly_name("/usr/lib/electron37/electron /usr/lib/bitwarden/app.asar"),
            "Bitwarden"
        );
        // A `resources/` wrapper is skipped to reach the app dir.
        assert_eq!(
            friendly_name("/usr/lib/electron/electron /opt/Foo/resources/app.asar --type=renderer"),
            "Foo — renderer"
        );
        // Child processes carry no .asar → stay generic.
        assert_eq!(
            friendly_name("/usr/lib/electron37/electron --type=zygote"),
            "electron — zygote"
        );
    }

    #[test]
    fn non_browser_passes_through() {
        assert_eq!(friendly_name("/usr/bin/htop"), "/usr/bin/htop");
    }
}