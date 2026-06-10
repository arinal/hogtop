mod plain;
mod tui;

use anyhow::Result;

use crate::app::{App, Row as AppRow};
use crate::classifier::Platform;

pub use plain::PlainRenderer;
pub use tui::{map_key, TuiRenderer};

/// Port (algebra) for presenting `App` state. Each interpreter owns its sink.
pub trait Renderer {
    fn present(&mut self, app: &App) -> Result<()>;

    /// Copy `text` to the system clipboard. Default is a no-op — only the
    /// interactive TUI (which owns a terminal that can carry the bytes) needs it.
    fn copy_clipboard(&mut self, _text: &str) -> Result<()> {
        Ok(())
    }
}

/// The PID column: the process's pid, or — for a group — the leader pid, the
/// minimum pid in the group (a stable proxy for the oldest/main process). The
/// group's proc count now rides in the [`grouped_badge`], not here.
fn pid_cell(row: &AppRow) -> String {
    row.pids.iter().min().map(|p| p.to_string()).unwrap_or_default()
}

/// The badges shown before a row's label, collected in order
/// `platform · identity · grouped` and deduped by glyph. Each source is an
/// independent resolver returning an optional badge; adding a new fact (e.g.
/// `root`) is just one more producer here. Deduping is a safety net for when two
/// resolvers land on the same glyph — after the platform/identity split they no
/// longer overlap, but a future producer might.
fn badges(row: &AppRow, nerd_font: bool) -> Vec<String> {
    let candidates = [
        platform_badge(row.platform, nerd_font).map(str::to_string),
        icon_for(&row.label, nerd_font).map(str::to_string),
        grouped_badge(row),
    ];
    let mut out: Vec<String> = Vec::new();
    for badge in candidates.into_iter().flatten() {
        if !out.contains(&badge) {
            out.push(badge);
        }
    }
    out
}

/// Identity badge: a glyph for a *specific* app or tool, matched on the display
/// name, or `None` when nothing matches. Runtime families that the [`Platform`]
/// enum already models (`java`, `python`, `node`, `chrome`, `firefox`) are NOT
/// matched here — they come from [`platform_badge`]. Languages whose compiled
/// binaries are undetectable as a family (`rust`, `go`, `ruby`) stay here,
/// matched on their toolchain names. With `nerd_font`, returns a Nerd Font glyph
/// instead of the emoji; those codepoints assume a v3 patched font (the `nf-`
/// names are in the Nerd Fonts cheat sheet, https://www.nerdfonts.com/cheat-sheet).
fn icon_for(name: &str, nerd_font: bool) -> Option<&'static str> {
    let l = name.to_ascii_lowercase();
    let l = l.as_str();
    // Match the toolchain, not a bare "rust" — labels can be full paths, and a
    // process living under a `…/rust/…` directory is not a Rust program.
    if l.contains("cargo") || l.contains("rustc") {
        return Some(if nerd_font { "\u{e7a8}" } else { "🦀" }); // nf-dev-rust
    }
    if l.contains("go") && (l == "go" || l.contains("go build") || l.contains("gopls")) {
        return Some(if nerd_font { "\u{e724}" } else { "🔵" }); // nf-dev-go
    }
    if l.contains("ruby") || l.contains("irb") || l.contains("rails") || l.contains("bundle") {
        return Some(if nerd_font { "\u{e739}" } else { "💎" }); // nf-dev-ruby
    }
    if l == "code" || l == "codium" || l.contains("vs code") || l == "cursor" {
        return Some(if nerd_font { "\u{e70c}" } else { "💻" }); // nf-dev-visualstudio
    }
    if l.contains("vim") {
        // Catches vim, nvim, neovim, gvim — "vim" is a substring of them all.
        return Some(if nerd_font { "\u{e62b}" } else { "📝" }); // nf-custom-vim
    }
    if l.contains("docker") || l.contains("containerd") || l.contains("podman") {
        return Some(if nerd_font { "\u{e7b0}" } else { "🐳" }); // nf-dev-docker
    }
    if l.contains("slack") {
        return Some(if nerd_font { "\u{f198}" } else { "💬" }); // nf-fa-slack
    }
    if l.contains("discord") {
        return Some(if nerd_font { "\u{f11b}" } else { "🎮" }); // nf-fa-gamepad
    }
    if l.contains("spotify") {
        return Some(if nerd_font { "\u{f1bc}" } else { "🎵" }); // nf-fa-spotify
    }
    if l.contains("bitwarden") {
        return Some(if nerd_font { "\u{f023}" } else { "🔐" }); // nf-fa-lock
    }
    if l.contains("obsidian") {
        return Some(if nerd_font { "\u{e26e}" } else { "🟣" }); // nf-md-language_markdown
    }
    if l.contains("signal") {
        return Some(if nerd_font { "\u{f0f3}" } else { "💬" }); // nf-fa-bell
    }
    if l.contains("teams") {
        return Some(if nerd_font { "\u{f0871}" } else { "👥" }); // nf-md-microsoft_teams
    }
    None
}

/// Platform badge: a glyph for the row's runtime family, straight from the
/// [`Platform`] fact. `None` for [`Platform::Other`] (nothing to say). The
/// Electron `⚛` originates here — it is no longer an [`icon_for`] fallback.
fn platform_badge(platform: Platform, nerd_font: bool) -> Option<&'static str> {
    Some(match platform {
        Platform::Java => if nerd_font { "\u{e738}" } else { "☕" }, // nf-dev-java
        Platform::Python => if nerd_font { "\u{e73c}" } else { "🐍" }, // nf-dev-python
        Platform::Node => if nerd_font { "\u{e718}" } else { "🟢" }, // nf-dev-nodejs_small
        Platform::Chrome => if nerd_font { "\u{e743}" } else { "🌐" }, // nf-dev-chrome
        Platform::Firefox => if nerd_font { "\u{e745}" } else { "🦊" }, // nf-dev-firefox
        Platform::Electron => if nerd_font { "\u{f5d2}" } else { "⚛" }, // nf-fa-atom
        Platform::Other => return None,
    })
}

/// Grouped badge: an `N×` count for an aggregated row, `None` for a single
/// process. The count lives here now — the PID column shows the leader pid.
fn grouped_badge(row: &AppRow) -> Option<String> {
    row.is_group.then(|| format!("{}×", row.pids.len()))
}

#[cfg(test)]
mod tests {
    use super::{badges, grouped_badge, icon_for, pid_cell, platform_badge};
    use crate::app::Row;
    use crate::classifier::Platform;
    use sysinfo::Pid;

    fn row(label: &str, platform: Platform, pids: &[usize], is_group: bool) -> Row {
        Row {
            label: label.to_string(),
            exe: String::new(),
            cpu_pct: 0.0,
            avg_memory_bytes: 0,
            pids: pids.iter().map(|&p| Pid::from(p)).collect(),
            is_group,
            is_new: false,
            platform,
        }
    }

    #[test]
    fn identity_matches_specific_apps_only() {
        assert_eq!(icon_for("Slack", false), Some("💬"));
        assert_eq!(icon_for("cargo build", false), Some("🦀"));
        // Families the Platform enum owns are NOT identity matches anymore.
        assert_eq!(icon_for("python3", false), None);
        assert_eq!(icon_for("chrome", false), None);
        assert_eq!(icon_for("some random process", false), None);
    }

    #[test]
    fn platform_badge_maps_known_families() {
        assert_eq!(platform_badge(Platform::Java, false), Some("☕"));
        assert_eq!(platform_badge(Platform::Python, false), Some("🐍"));
        assert_eq!(platform_badge(Platform::Node, false), Some("🟢"));
        assert_eq!(platform_badge(Platform::Electron, false), Some("⚛"));
        assert_eq!(platform_badge(Platform::Other, false), None);
    }

    #[test]
    fn grouped_badge_counts_only_groups() {
        assert_eq!(
            grouped_badge(&row("x", Platform::Other, &[1, 2, 3], true)).as_deref(),
            Some("3×")
        );
        assert_eq!(grouped_badge(&row("x", Platform::Other, &[1], false)), None);
    }

    #[test]
    fn badges_collected_in_order() {
        // Electron app, grouped: platform · identity · grouped.
        assert_eq!(
            badges(&row("Slack", Platform::Electron, &[12, 10, 11], true), false),
            vec!["⚛".to_string(), "💬".to_string(), "3×".to_string()]
        );
        // No recognized facts → no badges (no padding).
        assert!(badges(&row("htop", Platform::Other, &[5], false), false).is_empty());
        // A Python process: platform badge only.
        assert_eq!(
            badges(&row("/usr/bin/python3 x.py", Platform::Python, &[5], false), false),
            vec!["🐍".to_string()]
        );
    }

    #[test]
    fn badges_are_unique() {
        // Dedupe guarantee: a row never shows the same glyph twice. Identity and
        // platform altitudes are disjoint today, so this is a safety net.
        let b = badges(&row("Spark", Platform::Java, &[1, 2], true), false);
        let mut deduped = b.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(deduped.len(), b.len());
    }

    #[test]
    fn pid_cell_shows_leader_pid_for_group() {
        // Group: the minimum pid, regardless of insertion order.
        assert_eq!(pid_cell(&row("Chrome", Platform::Chrome, &[4200, 4127, 4300], true)), "4127");
        // Single process: its own pid.
        assert_eq!(pid_cell(&row("htop", Platform::Other, &[8891], false)), "8891");
    }
}