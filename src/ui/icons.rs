//! The glyph set — infra implementing the [`IconSet`](super::IconSet) port.
//!
//! This is the *only* place codepoints live. It sits in the outer (UI) ring and
//! maps domain facts — [`Platform`] and [`AppId`] — to glyphs, plus a keyword
//! heuristic ([`tool`](Glyphs::tool)) for compiled binaries the domain can't
//! identify (a `cargo`/`go`/`vim` process is not an [`AppId`]).
//!
//! Both columns (emoji and Nerd Font) sit on one line per fact, so a glyph and
//! its variant can never drift apart. A single [`Glyphs`] impl selects the
//! column by its [`GlyphStyle`] — the honest shape of an emoji-vs-nerd choice,
//! behind the port so the renderers depend on the trait, not the table. The
//! Nerd Font codepoints assume a v3 patched font (the `nf-` names are in the
//! cheat sheet, https://www.nerdfonts.com/cheat-sheet).

use super::IconSet;
use crate::app::Row;
use crate::classifier::{AppId, Platform};

/// Which glyph column [`Glyphs`] serves.
#[derive(Clone, Copy)]
pub enum GlyphStyle {
    Emoji,
    NerdFont,
}

/// The concrete icon set. Holds only the chosen [`GlyphStyle`]; every lookup is
/// a table read with the column picked by `self.pick`.
pub struct Glyphs {
    style: GlyphStyle,
}

impl Glyphs {
    /// Build from the `--nerd-font` flag.
    pub fn new(nerd_font: bool) -> Self {
        let style = if nerd_font { GlyphStyle::NerdFont } else { GlyphStyle::Emoji };
        Self { style }
    }

    /// Select the column for this style from a co-located `(emoji, nerd)` pair.
    fn pick(&self, glyphs: (&'static str, &'static str)) -> &'static str {
        match self.style {
            GlyphStyle::Emoji => glyphs.0,
            GlyphStyle::NerdFont => glyphs.1,
        }
    }
}

impl IconSet for Glyphs {
    /// Platform badge: a glyph for the row's runtime family, straight from the
    /// [`Platform`] fact. `None` for [`Platform::Other`] (nothing to say).
    fn platform(&self, platform: Platform) -> Option<&'static str> {
        Some(self.pick(match platform {
            Platform::Java => ("☕", ""),    // nf-dev-java
            Platform::Python => ("🐍", ""),  // nf-dev-python
            Platform::Node => ("🟢", ""),    // nf-dev-nodejs_small
            Platform::Chrome => ("🌐", ""),  // nf-dev-chrome
            Platform::Firefox => ("🦊", ""), // nf-dev-firefox
            Platform::Electron => ("⚛", ""), // nf-fa-atom
            Platform::Shell => ("🐚", ""),   // nf-oct-terminal
            Platform::Kernel => ("⚙", ""),   // nf-fa-cog
            Platform::Other => return None,
        }))
    }

    /// App badge: a glyph for a recognised app, keyed off the [`AppId`] domain
    /// fact — never a command-line substring. The `match` is exhaustive, so a
    /// new [`AppId`] variant without a glyph here is a compile error.
    fn app(&self, app: AppId) -> Option<&'static str> {
        Some(self.pick(match app {
            AppId::VsCode => ("💻", ""),    // nf-dev-visualstudio
            AppId::Cursor => ("💻", ""),    // nf-dev-visualstudio
            AppId::Slack => ("💬", ""),     // nf-fa-slack
            AppId::Discord => ("🎮", ""),   // nf-fa-gamepad
            AppId::Signal => ("💬", ""),    // nf-fa-bell
            AppId::Obsidian => ("🟣", ""),  // nf-md-language_markdown
            AppId::Spotify => ("🎵", ""),   // nf-fa-spotify
            AppId::Teams => ("👥", "󰡱"),    // nf-md-microsoft_teams
            AppId::Bitwarden => ("🔐", ""), // nf-fa-lock
            AppId::Claude => ("✳", ""),     // nf-fa-asterisk (Claude's sunburst)
        }))
    }

    /// Toolchain heuristic: a glyph for a compiled binary or tool that no
    /// [`AppId`]/[`Platform`] fact identifies, matched on the display `label`.
    /// This is the keyword-matching of last resort — only for things with no
    /// better structural signal (Rust/Go/Ruby binaries reveal nothing about
    /// their language from the process alone). Named GUI apps live in [`app`](Self::app).
    fn tool(&self, label: &str) -> Option<&'static str> {
        let l = label.to_ascii_lowercase();
        let l = l.as_str();
        // Match the toolchain, not a bare "rust" — labels can be full paths, and
        // a process under a `…/rust/…` directory is not a Rust program.
        let glyphs = if l.contains("cargo") || l.contains("rustc") {
            ("🦀", "") // nf-dev-rust
        } else if l.contains("go") && (l == "go" || l.contains("go build") || l.contains("gopls")) {
            ("🔵", "") // nf-dev-go
        } else if l.contains("ruby") || l.contains("irb") || l.contains("rails") || l.contains("bundle") {
            ("💎", "") // nf-dev-ruby
        } else if l.contains("vim") {
            // Catches vim, nvim, neovim, gvim — "vim" is a substring of them all.
            ("📝", "") // nf-custom-vim
        } else if l.contains("docker") || l.contains("containerd") || l.contains("podman") {
            ("🐳", "") // nf-dev-docker
        } else if l.contains("claude") {
            // The `claude` CLI (Claude Code) — a Node tool, not the desktop app.
            ("✳", "") // nf-fa-asterisk (Claude's sunburst)
        } else {
            return None;
        };
        Some(self.pick(glyphs))
    }

    /// Powerline half-circles that round a filled badge into a pill, but only
    /// for Nerd Font; emoji terminals have no such glyphs, so they get a square
    /// block chip instead. `` is the left solid half-circle (nf-pl) and
    /// `` the right — drawn in the chip color over the row background,
    /// they cap the filled body with rounded ends.
    fn badge_caps(&self) -> Option<(&'static str, &'static str)> {
        match self.style {
            GlyphStyle::NerdFont => Some(("", "")),
            GlyphStyle::Emoji => None,
        }
    }
}

/// Grouped badge: an `N×` count for an aggregated row, `None` for a single
/// process. The count lives here; the PID column shows the leader pid.
pub(super) fn grouped_badge(row: &Row) -> Option<String> {
    row.is_group.then(|| format!("{}×", row.pids.len()))
}

#[cfg(test)]
mod tests {
    use super::Glyphs;
    use crate::classifier::{AppId, Platform};
    use crate::ui::IconSet;

    #[test]
    fn platform_maps_known_families() {
        let g = Glyphs::new(false);
        assert_eq!(g.platform(Platform::Java), Some("☕"));
        assert_eq!(g.platform(Platform::Python), Some("🐍"));
        assert_eq!(g.platform(Platform::Electron), Some("⚛"));
        assert_eq!(g.platform(Platform::Shell), Some("🐚"));
        assert_eq!(g.platform(Platform::Kernel), Some("⚙"));
        assert_eq!(g.platform(Platform::Other), None);
    }

    #[test]
    fn app_maps_every_variant() {
        let g = Glyphs::new(false);
        assert_eq!(g.app(AppId::Slack), Some("💬"));
        assert_eq!(g.app(AppId::Spotify), Some("🎵"));
        assert_eq!(g.app(AppId::VsCode), Some("💻"));
        assert_eq!(g.app(AppId::Claude), Some("✳"));
    }

    #[test]
    fn tool_matches_toolchains_only() {
        let g = Glyphs::new(false);
        assert_eq!(g.tool("cargo build"), Some("🦀"));
        assert_eq!(g.tool("nvim foo.rs"), Some("📝"));
        // The `claude` CLI (Claude Code) is a Node tool, iconned by keyword.
        assert_eq!(g.tool("node /usr/bin/claude"), Some("✳"));
        // Runtimes the Platform enum owns are NOT tool matches.
        assert_eq!(g.tool("python3"), None);
        assert_eq!(g.tool("some random process"), None);
    }

    #[test]
    fn nerd_font_selects_the_other_column() {
        let emoji = Glyphs::new(false);
        let nerd = Glyphs::new(true);
        // Same fact, different column — and never the same string.
        assert_ne!(emoji.app(AppId::Slack), nerd.app(AppId::Slack));
        assert_eq!(nerd.app(AppId::Slack), Some(""));
    }

    #[test]
    fn badge_caps_only_for_nerd_font() {
        // Powerline half-circles round the chip — but only Nerd Font has them.
        assert_eq!(Glyphs::new(true).badge_caps(), Some(("", "")));
        assert_eq!(Glyphs::new(false).badge_caps(), None);
    }
}