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

/// The PID column: the pid for a single process, or a proc count for a group.
fn pid_cell(row: &AppRow) -> String {
    if row.is_group {
        format!("{}×", row.pids.len())
    } else {
        row.pids.first().map(|p| p.to_string()).unwrap_or_default()
    }
}

/// A leading glyph for a display name — pure presentation, matched on the name
/// text so it works for grouped, ungrouped, and non-app rows alike. Falls back
/// to the row's [`Platform`] for families we can't spot by name (e.g. an
/// Electron app named only by its `.asar` path). With `nerd_font`, returns a
/// Nerd Font glyph instead of the emoji; those codepoints assume a v3 patched
/// font (the `nf-` names are in the Nerd Fonts cheat sheet,
/// https://www.nerdfonts.com/cheat-sheet).
fn icon_for(name: &str, platform: Platform, nerd_font: bool) -> &'static str {
    let l = name.to_ascii_lowercase();
    let l = l.as_str();
    if l.contains("java") || l.contains("sbt") || l.contains("gradle") || l.contains("maven")
        || l.contains("bloop") || l.contains("metals") || l.contains("scala")
        || l.contains("kotlin") || l.contains("spark") || l.contains("kafka")
        || l.contains("elasticsearch") || l.contains("zookeeper")
    {
        return if nerd_font { "\u{e738}" } else { "☕" }; // nf-dev-java
    }
    if l.contains("python") || l == "python3" || l == "python2" || l == "pip" || l == "ipython" {
        return if nerd_font { "\u{e73c}" } else { "🐍" }; // nf-dev-python
    }
    if l.contains("node") || l.contains("npm") || l.contains("deno") || l.contains("bun") {
        return if nerd_font { "\u{e718}" } else { "🟢" }; // nf-dev-nodejs_small
    }
    if l.contains("rust") || l.contains("cargo") || l.contains("rustc") {
        return if nerd_font { "\u{e7a8}" } else { "🦀" }; // nf-dev-rust
    }
    if l.contains("chrome") || l.contains("chromium") {
        return if nerd_font { "\u{e743}" } else { "🌐" }; // nf-dev-chrome
    }
    if l.contains("firefox") {
        return if nerd_font { "\u{e745}" } else { "🦊" }; // nf-dev-firefox
    }
    if l == "code" || l == "codium" || l.contains("vs code") || l == "cursor" {
        return if nerd_font { "\u{e70c}" } else { "💻" }; // nf-dev-visualstudio
    }
    if l.contains("vim") {
        // Catches vim, nvim, neovim, gvim — "vim" is a substring of them all.
        return if nerd_font { "\u{e62b}" } else { "📝" }; // nf-custom-vim
    }
    if l.contains("docker") || l.contains("containerd") || l.contains("podman") {
        return if nerd_font { "\u{e7b0}" } else { "🐳" }; // nf-dev-docker
    }
    if l.contains("go") && (l == "go" || l.contains("go build") || l.contains("gopls")) {
        return if nerd_font { "\u{e724}" } else { "🔵" }; // nf-dev-go
    }
    if l.contains("ruby") || l.contains("irb") || l.contains("rails") || l.contains("bundle") {
        return if nerd_font { "\u{e739}" } else { "💎" }; // nf-dev-ruby
    }
    if l.contains("slack") {
        return if nerd_font { "\u{f198}" } else { "💬" }; // nf-fa-slack
    }
    if l.contains("discord") {
        return if nerd_font { "\u{f11b}" } else { "🎮" }; // nf-fa-gamepad
    }
    if l.contains("spotify") {
        return if nerd_font { "\u{f1bc}" } else { "🎵" }; // nf-fa-spotify
    }
    if l.contains("bitwarden") {
        return if nerd_font { "\u{f023}" } else { "🔐" }; // nf-fa-lock
    }
    if l.contains("obsidian") {
        return if nerd_font { "\u{e26e}" } else { "🟣" }; // nf-md-language_markdown
    }
    if l.contains("signal") {
        return if nerd_font { "\u{f0f3}" } else { "💬" }; // nf-fa-bell
    }
    if l.contains("teams") {
        return if nerd_font { "\u{f0871}" } else { "👥" }; // nf-md-microsoft_teams
    }
    // Family fallback for anything not matched by name above — notably an
    // unrecognised Electron app named only by its .asar path.
    if let Platform::Electron = platform {
        return if nerd_font { "\u{f5d2}" } else { "⚛" }; // nf-fa-atom
    }
    if nerd_font { " " } else { "  " } // no match: blank (nerd glyphs are single-width)
}