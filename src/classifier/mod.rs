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

/// The [`Platform`] this argv belongs to, or [`Platform::Other`] if no family
/// recognises it.
pub fn platform(argv: &[&str]) -> Platform {
    let Some(first) = argv.first() else {
        return Platform::Other;
    };
    let exe = exe_basename(first);
    family_for(&exe, argv).map_or(Platform::Other, |f| f.platform())
}

/// Turn structured argv into a plain display name (process-kind, no icon). The
/// icon is a presentation concern added by the UI at render time.
pub fn friendly_name(argv: &[&str]) -> String {
    let Some(first) = argv.first() else {
        return String::new();
    };
    let exe = exe_basename(first);
    match family_for(&exe, argv) {
        Some(f) => f.label(&exe, argv),
        None => argv.join(" "),
    }
}

/// The group identity this argv yields *on its own* (Chrome/Chromium, Firefox,
/// or an Electron app). `None` for generic Electron children — which carry no
/// identity and inherit it from an ancestor — and for non-groupable or
/// unrecognised processes. Self-identifying families (Chromium, Firefox) yield
/// a name on *every* process, so they never need inheritance.
pub fn group_app(argv: &[&str]) -> Option<String> {
    let exe = exe_basename(argv.first()?);
    match family_for(&exe, argv) {
        Some(f) if f.groupable() => f.group(&exe, argv),
        _ => None,
    }
}

/// Whether the argv belongs to a groupable multi-process app family. A
/// groupable process with no `group_app` of its own is a generic Electron child
/// that inherits its name from an ancestor (a shared-runtime zygote).
pub fn is_groupable_family(argv: &[&str]) -> bool {
    let Some(first) = argv.first() else {
        return false;
    };
    let exe = exe_basename(first);
    family_for(&exe, argv).is_some_and(|f| f.groupable())
}

/// Compose a label for a process that inherits `app` from an ancestor, using
/// this process's own `--type` detail (e.g. inherited "Bitwarden" + own
/// "zygote" → "Bitwarden — zygote").
pub fn inherited_label(app: &str, argv: &[&str]) -> String {
    // Generic children are Electron's case, and Electron uses Chromium's --type=.
    match ChromiumClassifier::proc_type(argv) {
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
    /// Does this argv belong to this family?
    fn matches(&self, exe: &str, argv: &[&str]) -> bool;

    /// Human display label for this process (e.g. "Chrome — renderer").
    fn label(&self, exe: &str, argv: &[&str]) -> String;

    /// The runtime family this classifier represents.
    fn platform(&self) -> Platform;

    /// Whether this family's processes roll up into a single app row.
    fn groupable(&self) -> bool {
        false
    }

    /// App-group identity derivable from this argv alone, when
    /// [`groupable`](Self::groupable). `None` means "in the family but
    /// anonymous" — a generic child that inherits its name from an ancestor
    /// (only shared-runtime Electron children hit this).
    fn group(&self, _exe: &str, _argv: &[&str]) -> Option<String> {
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

fn family_for(exe: &str, argv: &[&str]) -> Option<&'static dyn Classifier> {
    FAMILIES.iter().copied().find(|f| f.matches(exe, argv))
}

pub(crate) fn exe_basename(arg: &str) -> String {
    Path::new(arg)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| arg.to_string())
}

/// The stem of the *outermost* `.app` directory in `path` — closest to the root.
/// On macOS this is the parent app's name regardless of how deeply the helper
/// bundle is nested: `.../Spotify.app/Contents/Frameworks/Spotify Helper.app/...`
/// → `"Spotify"`. `None` on Linux paths (no `.app` segments).
pub(crate) fn outer_app_bundle(path: &str) -> Option<String> {
    Path::new(path).components().find_map(|c| {
        let s = c.as_os_str().to_str()?;
        s.strip_suffix(".app").map(str::to_string)
    })
}

#[cfg(test)]
mod tests {
    use super::friendly_name;

    /// Test helper: split a Linux-style cmdline (no spaces in argv[0]) into
    /// argv. macOS-style tests construct vectors directly.
    fn argv(s: &str) -> Vec<&str> {
        s.split_whitespace().collect()
    }

    const CHROME: &str = "/opt/google/chrome/chrome";

    #[test]
    fn chrome_process_types() {
        assert_eq!(
            friendly_name(&argv(&format!(
                "{CHROME} --type=renderer --renderer-client-id=44"
            ))),
            "Chrome — renderer"
        );
        assert_eq!(
            friendly_name(&argv(&format!(
                "{CHROME} --type=renderer --extension-process --renderer-client-id=7"
            ))),
            "Chrome — renderer (extension)"
        );
        assert_eq!(
            friendly_name(&argv(&format!(
                "{CHROME} --type=gpu-process --ozone-platform=wayland"
            ))),
            "Chrome — GPU"
        );
        assert_eq!(
            friendly_name(&argv(&format!("{CHROME} --type=zygote"))),
            "Chrome — zygote"
        );
        assert_eq!(
            friendly_name(&argv(&format!("{CHROME} --type=sandbox-ipc"))),
            "Chrome — sandbox IPC"
        );
        // Browser/main process: no --type= switch.
        assert_eq!(friendly_name(&argv(CHROME)), "Chrome — browser");
    }

    #[test]
    fn chrome_utility_subtypes() {
        let case = |svc: &str| {
            friendly_name(&argv(&format!(
                "{CHROME} --type=utility --utility-sub-type={svc} --lang=en-US"
            )))
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
            friendly_name(&argv("/usr/bin/chromium --type=gpu-process")),
            "Chromium — GPU"
        );
    }

    #[test]
    fn firefox_dialect() {
        assert_eq!(friendly_name(&argv("/usr/lib/firefox/firefox")), "Firefox");
        assert_eq!(
            friendly_name(&argv(
                "/usr/lib/firefox/firefox -contentproc -parentPid 100 7 tab"
            )),
            "Firefox — tab"
        );
        assert_eq!(
            friendly_name(&argv(
                "/usr/lib/firefox/firefox -contentproc -parentPid 100 4 rdd"
            )),
            "Firefox — RDD"
        );
        assert_eq!(
            friendly_name(&argv(
                "/usr/lib/firefox/firefox -contentproc -parentPid 100 1 forkserver"
            )),
            "Firefox — fork server"
        );
    }

    #[test]
    fn electron_apps_reuse_taxonomy() {
        assert_eq!(
            friendly_name(&argv("/usr/share/code/code --type=renderer")),
            "VS Code — renderer"
        );
        assert_eq!(
            friendly_name(&argv("/usr/bin/slack --type=gpu-process")),
            "Slack — GPU"
        );
        // Electron main process → just the app name.
        assert_eq!(friendly_name(&argv("/usr/share/code/code")), "VS Code");
    }

    #[test]
    fn electron_shared_runtime_named_from_asar() {
        // Shared electron runtime: app identity comes from the .asar path.
        assert_eq!(
            friendly_name(&argv(
                "/usr/lib/electron37/electron /usr/lib/bitwarden/app.asar"
            )),
            "Bitwarden"
        );
        // A `resources/` wrapper is skipped to reach the app dir.
        assert_eq!(
            friendly_name(&argv(
                "/usr/lib/electron/electron /opt/Foo/resources/app.asar --type=renderer"
            )),
            "Foo — renderer"
        );
        // Child processes carry no .asar → stay generic.
        assert_eq!(
            friendly_name(&argv("/usr/lib/electron37/electron --type=zygote")),
            "electron — zygote"
        );
    }

    /// On macOS, app binaries live at `/Applications/<App>.app/Contents/MacOS/<App>`
    /// where both `<App>` and the path contain spaces. argv comes in pre-tokenised
    /// from sysinfo, so the basename — and the bundle name — survive intact.
    #[test]
    fn macos_chrome_bundle_paths() {
        // Main browser process.
        assert_eq!(
            friendly_name(&[
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
            ]),
            "Chrome — browser"
        );
        // Child renderer in the nested helper bundle.
        assert_eq!(
            friendly_name(&[
                "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Versions/148.0.7778.179/Helpers/Google Chrome Helper (Renderer).app/Contents/MacOS/Google Chrome Helper (Renderer)",
                "--type=renderer",
                "--renderer-client-id=7"
            ]),
            "Chrome — renderer"
        );
        // Chromium.app is recognised the same way.
        assert_eq!(
            friendly_name(&[
                "/Applications/Chromium.app/Contents/MacOS/Chromium",
                "--type=gpu-process"
            ]),
            "Chromium — GPU"
        );
    }

    #[test]
    fn macos_electron_bundle_paths() {
        // Spotify main process — the helper bundle lives several `.app` levels
        // deep, but the outermost `.app` is what names the app.
        assert_eq!(
            friendly_name(&[
                "/Users/me/Applications/Spotify.app/Contents/MacOS/Spotify",
                "--autostart"
            ]),
            "Spotify"
        );
        assert_eq!(
            friendly_name(&[
                "/Users/me/Applications/Spotify.app/Contents/Frameworks/Spotify Helper (Renderer).app/Contents/MacOS/Spotify Helper (Renderer)",
                "--type=renderer"
            ]),
            "Spotify — renderer"
        );
        // Slack and VS Code use the same `<App> Helper` convention.
        assert_eq!(
            friendly_name(&[
                "/Applications/Slack.app/Contents/Frameworks/Slack Helper (GPU).app/Contents/MacOS/Slack Helper (GPU)",
                "--type=gpu-process"
            ]),
            "Slack — GPU"
        );
        assert_eq!(
            friendly_name(&[
                "/Applications/Visual Studio Code.app/Contents/MacOS/Electron"
            ]),
            "VS Code"
        );
    }

    #[test]
    fn non_browser_passes_through() {
        assert_eq!(friendly_name(&argv("/usr/bin/htop")), "/usr/bin/htop");
    }

    /// Grouping contract: every process in the same app must yield the same
    /// `group_app` key, regardless of which child role it plays. If this
    /// fails, app rows fragment into one-per-process in the UI.
    mod grouping {
        use super::super::group_app;

        fn assert_all_grouped(name: &str, expected: &str, cases: &[&[&str]]) {
            for argv in cases {
                let got = group_app(argv);
                assert_eq!(
                    got.as_deref(),
                    Some(expected),
                    "{name}: argv {argv:?} grouped as {got:?}, expected {expected:?}"
                );
            }
        }

        #[test]
        fn linux_chrome_processes_share_group() {
            // Linux: argv[0] is a single token, no spaces.
            assert_all_grouped(
                "Linux Chrome",
                "Chrome",
                &[
                    &["/opt/google/chrome/chrome"],
                    &["/opt/google/chrome/chrome", "--type=renderer", "--renderer-client-id=44"],
                    &["/opt/google/chrome/chrome", "--type=gpu-process"],
                    &["/opt/google/chrome/chrome", "--type=zygote"],
                    &[
                        "/opt/google/chrome/chrome",
                        "--type=utility",
                        "--utility-sub-type=network.mojom.NetworkService",
                    ],
                ],
            );
        }

        #[test]
        fn linux_chromium_processes_share_group() {
            assert_all_grouped(
                "Linux Chromium",
                "Chromium",
                &[
                    &["/usr/bin/chromium"],
                    &["/usr/bin/chromium", "--type=renderer"],
                    &["/usr/bin/chromium-browser", "--type=gpu-process"],
                ],
            );
        }

        #[test]
        fn linux_electron_app_processes_share_group() {
            // VS Code: known Electron app, lower-case exe.
            assert_all_grouped(
                "Linux VS Code",
                "VS Code",
                &[
                    &["/usr/share/code/code"],
                    &["/usr/share/code/code", "--type=renderer"],
                    &["/usr/share/code/code", "--type=gpu-process"],
                ],
            );
            // Slack.
            assert_all_grouped(
                "Linux Slack",
                "Slack",
                &[
                    &["/usr/bin/slack"],
                    &["/usr/bin/slack", "--type=renderer"],
                    &["/usr/bin/slack", "--type=zygote"],
                ],
            );
        }

        #[test]
        fn linux_firefox_processes_share_group() {
            assert_all_grouped(
                "Linux Firefox",
                "Firefox",
                &[
                    &["/usr/lib/firefox/firefox"],
                    &["/usr/lib/firefox/firefox", "-contentproc", "-parentPid", "100", "7", "tab"],
                    &["/usr/lib/firefox/firefox", "-contentproc", "-parentPid", "100", "4", "rdd"],
                ],
            );
        }

        /// macOS argv[0] is a path *with spaces* into a `.app` bundle, plus
        /// extra-deep nesting for helper bundles. The whole point of carrying
        /// argv as a slice (instead of a joined string) is so these still
        /// group correctly.
        #[test]
        fn macos_chrome_processes_share_group() {
            assert_all_grouped(
                "macOS Chrome",
                "Chrome",
                &[
                    // Main browser process — argv[0] contains a space.
                    &["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"],
                    // Renderer in the nested helper bundle.
                    &[
                        "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Versions/148.0.7778.179/Helpers/Google Chrome Helper (Renderer).app/Contents/MacOS/Google Chrome Helper (Renderer)",
                        "--type=renderer",
                        "--renderer-client-id=7",
                    ],
                    // GPU helper bundle.
                    &[
                        "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Versions/148.0.7778.179/Helpers/Google Chrome Helper (GPU).app/Contents/MacOS/Google Chrome Helper (GPU)",
                        "--type=gpu-process",
                    ],
                    // Plain helper (utility).
                    &[
                        "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Versions/148.0.7778.179/Helpers/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper",
                        "--type=utility",
                        "--utility-sub-type=network.mojom.NetworkService",
                    ],
                ],
            );
        }

        #[test]
        fn macos_chrome_canary_groups_under_chrome() {
            // Canary/Beta/Dev are still Chrome — same browser engine, same group.
            assert_all_grouped(
                "macOS Chrome Canary",
                "Chrome",
                &[
                    &["/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary"],
                    &[
                        "/Applications/Google Chrome Beta.app/Contents/MacOS/Google Chrome Beta",
                        "--type=renderer",
                    ],
                ],
            );
        }

        #[test]
        fn macos_chromium_processes_share_group() {
            assert_all_grouped(
                "macOS Chromium",
                "Chromium",
                &[
                    &["/Applications/Chromium.app/Contents/MacOS/Chromium"],
                    &["/Applications/Chromium.app/Contents/MacOS/Chromium", "--type=gpu-process"],
                ],
            );
        }

        #[test]
        fn macos_electron_app_processes_share_group() {
            // Spotify: nested helper bundles two `.app` levels deep.
            assert_all_grouped(
                "macOS Spotify",
                "Spotify",
                &[
                    &[
                        "/Users/me/Applications/Spotify.app/Contents/MacOS/Spotify",
                        "--autostart",
                    ],
                    &[
                        "/Users/me/Applications/Spotify.app/Contents/Frameworks/Spotify Helper.app/Contents/MacOS/Spotify Helper",
                        "--type=gpu-process",
                    ],
                    &[
                        "/Users/me/Applications/Spotify.app/Contents/Frameworks/Spotify Helper (Renderer).app/Contents/MacOS/Spotify Helper (Renderer)",
                        "--type=renderer",
                    ],
                    &[
                        "/Users/me/Applications/Spotify.app/Contents/Frameworks/Spotify Helper.app/Contents/MacOS/Spotify Helper",
                        "--type=utility",
                        "--utility-sub-type=network.mojom.NetworkService",
                    ],
                ],
            );
            // Slack: same `<App> Helper` convention.
            assert_all_grouped(
                "macOS Slack",
                "Slack",
                &[
                    &["/Applications/Slack.app/Contents/MacOS/Slack"],
                    &[
                        "/Applications/Slack.app/Contents/Frameworks/Slack Helper.app/Contents/MacOS/Slack Helper",
                        "--type=zygote",
                    ],
                    &[
                        "/Applications/Slack.app/Contents/Frameworks/Slack Helper (Renderer).app/Contents/MacOS/Slack Helper (Renderer)",
                        "--type=renderer",
                    ],
                ],
            );
            // VS Code: bundle name "Visual Studio Code" maps to display "VS Code".
            assert_all_grouped(
                "macOS VS Code",
                "VS Code",
                &[
                    &["/Applications/Visual Studio Code.app/Contents/MacOS/Electron"],
                    &[
                        "/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper.app/Contents/MacOS/Code Helper",
                        "--type=gpu-process",
                    ],
                ],
            );
        }

        /// Non-groupable / unrecognised processes must yield `None`. Otherwise
        /// they'd collapse into a bogus shared row.
        #[test]
        fn unrelated_processes_do_not_group() {
            assert_eq!(group_app(&["/usr/bin/htop"]), None);
            assert_eq!(group_app(&["/bin/zsh"]), None);
            assert_eq!(group_app(&["/usr/bin/ssh", "host"]), None);
            // Java is recognised as a family but not groupable.
            assert_eq!(
                group_app(&["/usr/bin/java", "-jar", "thing.jar"]),
                None
            );
        }
    }
}