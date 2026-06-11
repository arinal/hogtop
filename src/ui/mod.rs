mod icons;
mod plain;
mod tui;

use anyhow::Result;

use crate::app::{App, Row as AppRow};
use crate::classifier::{AppId, Platform};

pub use icons::Glyphs;
pub use plain::PlainRenderer;
pub use tui::{map_key, TuiRenderer};

use icons::grouped_badge;

/// Port (algebra) for presenting `App` state. Each interpreter owns its sink.
pub trait Renderer {
    fn present(&mut self, app: &App) -> Result<()>;

    /// Copy `text` to the system clipboard. Default is a no-op — only the
    /// interactive TUI (which owns a terminal that can carry the bytes) needs it.
    fn copy_clipboard(&mut self, _text: &str) -> Result<()> {
        Ok(())
    }
}

/// Port (algebra) for mapping domain facts to display glyphs. The renderers
/// depend on this trait; the concrete [`Glyphs`] infra (the emoji/Nerd-Font
/// table) is injected at construction, the same way [`Nix`](crate::control::Nix)
/// is injected for process control. Keeping the codepoints behind the port is
/// what stops presentation knowledge leaking back across the onion seam.
pub trait IconSet {
    /// Glyph for a runtime [`Platform`], or `None` for [`Platform::Other`].
    fn platform(&self, platform: Platform) -> Option<&'static str>;

    /// Glyph for a recognised [`AppId`] — keyed off the domain fact, not a
    /// command-line substring.
    fn app(&self, app: AppId) -> Option<&'static str>;

    /// Glyph for a tool identified only by a `label` keyword (compiled binaries
    /// with no [`AppId`]/[`Platform`] signal), or `None`.
    fn tool(&self, label: &str) -> Option<&'static str>;

    /// Rounded `(left, right)` cap glyphs that bracket a filled badge into a
    /// pill, or `None` when the set has no shape for it (emoji terminals lack
    /// the powerline half-circles, so they fall back to a plain block chip).
    fn badge_caps(&self) -> Option<(&'static str, &'static str)>;
}

/// The PID column: the process's pid, or — for a group — the leader pid, the
/// minimum pid in the group (a stable proxy for the oldest/main process). The
/// group's proc count now rides in the [`grouped_badge`], not here.
fn pid_cell(row: &AppRow) -> String {
    row.pids.iter().min().map(|p| p.to_string()).unwrap_or_default()
}

/// The badges shown before a row's label, collected in order
/// `platform · identity` and deduped by glyph. The grouped `N×` count is *not*
/// included here — it's produced by [`grouped_badge`] so renderers can style it
/// distinctly (e.g. the TUI gives it a background to read as a true badge).
/// Each source is an independent resolver returning an optional badge; adding a
/// new fact (e.g. `root`) is just one more producer here. Deduping is a safety
/// net for when two resolvers land on the same glyph — after the
/// platform/identity split they no longer overlap, but a future producer might.
///
/// Arrangement (order + dedupe) lives here, at the shared UI level; only the
/// fact→glyph lookup is delegated to the injected [`IconSet`]. The identity slot
/// prefers the [`AppId`] domain fact and falls back to the label-keyword
/// [`IconSet::tool`] heuristic only when the row is no recognised app.
fn badges(row: &AppRow, icons: &dyn IconSet) -> Vec<String> {
    let identity = match row.app {
        Some(app) => icons.app(app),
        None => icons.tool(&row.label),
    };
    let candidates = [
        icons.platform(row.platform).map(str::to_string),
        identity.map(str::to_string),
    ];
    let mut out: Vec<String> = Vec::new();
    for badge in candidates.into_iter().flatten() {
        if !out.contains(&badge) {
            out.push(badge);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{badges, pid_cell, Glyphs};
    use crate::app::Row;
    use crate::classifier::{AppId, Platform};
    use sysinfo::Pid;

    fn row(label: &str, platform: Platform, app: Option<AppId>, pids: &[usize], is_group: bool) -> Row {
        Row {
            label: label.to_string(),
            exe: String::new(),
            cpu_pct: 0.0,
            avg_memory_bytes: 0,
            pids: pids.iter().map(|&p| Pid::from(p)).collect(),
            is_group,
            is_new: false,
            platform,
            app,
        }
    }

    #[test]
    fn badges_collected_in_order() {
        let icons = Glyphs::new(false);
        // Electron app: platform · identity. Identity comes from the AppId fact
        // (Slack), not the label. The grouped count is no longer a badge here.
        assert_eq!(
            badges(&row("Slack", Platform::Electron, Some(AppId::Slack), &[12, 10, 11], true), &icons),
            vec!["⚛".to_string(), "💬".to_string()]
        );
        // No recognized facts → no badges (no padding).
        assert!(badges(&row("htop", Platform::Other, None, &[5], false), &icons).is_empty());
        // A Python process: platform badge only.
        assert_eq!(
            badges(&row("/usr/bin/python3 x.py", Platform::Python, None, &[5], false), &icons),
            vec!["🐍".to_string()]
        );
    }

    #[test]
    fn identity_prefers_app_fact_then_falls_back_to_tool() {
        let icons = Glyphs::new(false);
        // A recognised app uses its AppId glyph, ignoring the label entirely.
        assert_eq!(
            badges(&row("anything at all", Platform::Electron, Some(AppId::Spotify), &[1], false), &icons),
            vec!["⚛".to_string(), "🎵".to_string()]
        );
        // No app fact → fall back to the label-keyword tool heuristic.
        assert_eq!(
            badges(&row("cargo build", Platform::Other, None, &[1], false), &icons),
            vec!["🦀".to_string()]
        );
        // No app fact and no tool keyword → nothing.
        assert!(badges(&row("htop", Platform::Other, None, &[1], false), &icons).is_empty());
    }

    #[test]
    fn badges_are_unique() {
        // Dedupe guarantee: a row never shows the same glyph twice. Identity and
        // platform altitudes are disjoint today, so this is a safety net.
        let icons = Glyphs::new(false);
        let b = badges(&row("Spark", Platform::Java, None, &[1, 2], true), &icons);
        let mut deduped = b.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(deduped.len(), b.len());
    }

    #[test]
    fn pid_cell_shows_leader_pid_for_group() {
        // Group: the minimum pid, regardless of insertion order.
        assert_eq!(pid_cell(&row("Chrome", Platform::Chrome, None, &[4200, 4127, 4300], true)), "4127");
        // Single process: its own pid.
        assert_eq!(pid_cell(&row("htop", Platform::Other, None, &[8891], false)), "8891");
    }
}