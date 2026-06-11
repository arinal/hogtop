//! The registry of recognised multi-process desktop apps — the single source of
//! truth for "which `app.asar`/exe/`.app` bundle is which app." One declarative
//! row per app feeds three consumers:
//!
//! * Electron detection (`is_electron`) — is this exe/bundle a known app?
//! * Electron naming (`known_lowercase_exe`, `known_bundle`) — its display name.
//! * The UI icon set — its glyph, keyed off the [`AppId`] *domain fact*, not a
//!   command-line substring (which is why this lives in the classifier, the
//!   inner ring, while the glyph itself stays out in the UI).
//!
//! The macro emits both the `AppId` enum and the table from the same list, so a
//! variant can never drift from its data. Because `AppId` is a real enum, the
//! UI's glyph mapping is an exhaustive `match` — adding an app here without a
//! glyph in the UI is a compile error, not a silent blank.

/// Declares the [`AppId`] enum and the backing [`APPS`] table together. Each row
/// is `Variant => { name, exes, bundles }`:
///
/// * `name` — the display label (may contain spaces; the variant can't).
/// * `exes` — Linux exe basenames that identify the app.
/// * `bundles` — macOS `.app` bundle names that identify it (often differ from
///   the display name: "Visual Studio Code" → "VS Code").
macro_rules! known_apps {
    ($( $variant:ident => { name: $name:literal, exes: [$($exe:literal),* $(,)?], bundles: [$($bundle:literal),* $(,)?] } ),* $(,)?) => {
        /// A recognised desktop app — a domain fact carried on a process/row,
        /// the join key between classifier naming and UI glyph selection.
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum AppId {
            $($variant),*
        }

        impl AppId {
            /// The app's display name (e.g. [`AppId::VsCode`] → `"VS Code"`).
            pub fn name(self) -> &'static str {
                match self {
                    $( AppId::$variant => $name ),*
                }
            }
        }

        struct KnownApp {
            id: AppId,
            exes: &'static [&'static str],
            bundles: &'static [&'static str],
        }

        const APPS: &[KnownApp] = &[
            $( KnownApp {
                id: AppId::$variant,
                exes: &[$($exe),*],
                bundles: &[$($bundle),*],
            } ),*
        ];
    };
}

known_apps! {
    // VS Code & friends share the "code"/"codium" exe and the VS Code glyph.
    VsCode    => { name: "VS Code",   exes: ["code", "codium"],        bundles: ["Visual Studio Code", "Code", "Code - Insiders", "VSCodium"] },
    Cursor    => { name: "Cursor",    exes: ["cursor"],                bundles: ["Cursor"] },
    Slack     => { name: "Slack",     exes: ["slack"],                 bundles: ["Slack"] },
    // Some distros ship a capitalised `Discord` exe.
    Discord   => { name: "Discord",   exes: ["discord", "Discord"],    bundles: ["Discord"] },
    Signal    => { name: "Signal",    exes: ["signal-desktop"],        bundles: ["Signal"] },
    Obsidian  => { name: "Obsidian",  exes: ["obsidian"],              bundles: ["Obsidian"] },
    Spotify   => { name: "Spotify",   exes: ["spotify"],               bundles: ["Spotify"] },
    Teams     => { name: "Teams",     exes: ["teams"],                 bundles: ["Microsoft Teams", "Microsoft Teams (work or school)"] },
    // Bitwarden is only seen via its `app.asar` path / macOS bundle — no stable
    // top-level exe name — so it carries no `exes` entry.
    Bitwarden => { name: "Bitwarden", exes: [],                        bundles: ["Bitwarden"] },
}

/// The app whose Linux exe basename is `exe`, if any.
pub(crate) fn app_by_exe(exe: &str) -> Option<AppId> {
    APPS.iter().find(|a| a.exes.contains(&exe)).map(|a| a.id)
}

/// The app whose macOS `.app` bundle name is `bundle`, if any.
pub(crate) fn app_by_bundle(bundle: &str) -> Option<AppId> {
    APPS.iter().find(|a| a.bundles.contains(&bundle)).map(|a| a.id)
}

/// The [`AppId`] for a resolved display `name` (the inverse of [`AppId::name`]),
/// or `None` for a name no registry row owns — e.g. an `.asar`-derived app we
/// recognise structurally but don't have a glyph for, or a non-app group like
/// "Chrome" (which is a [`Platform`](super::Platform), not an `AppId`).
pub(crate) fn app_by_name(name: &str) -> Option<AppId> {
    APPS.iter().find(|a| a.id.name() == name).map(|a| a.id)
}