//! The interactive interpreter: owns the terminal and its raw-mode/alternate-
//! screen lifecycle, and drives a frame per `present`.

use std::io::{self, IsTerminal, Stdout, Write};
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    cursor, execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use super::render::render;
use super::theme::Palette;
use crate::app::App;
use crate::ui::{Glyphs, Renderer};

type TuiBackend = CrosstermBackend<Stdout>;

/// Interactive interpreter: owns the terminal and its raw-mode/alternate-screen
/// lifecycle. `new` sets it up; `Drop` restores it — so callers never touch
/// terminal plumbing.
pub struct TuiRenderer {
    terminal: Terminal<TuiBackend>,
    icons: Glyphs,
    palette: Palette,
}

impl TuiRenderer {
    /// `nerd_font`: `Some(true/false)` is an explicit choice (CLI flag or env);
    /// `None` means "auto-detect" — we probe the terminal and fall back to emoji
    /// when the answer is inconclusive.
    pub fn new(nerd_font: Option<bool>) -> Result<Self> {
        // Ask the terminal for its background color before we take over the
        // screen, so the distance fade can blend toward the real bg (light or
        // dark). If the terminal doesn't answer (e.g. piped, or unsupported),
        // fall back to assuming a dark theme.
        let bg = termbg::rgb(Duration::from_millis(100))
            .map(|c| ((c.r >> 8) as u8, (c.g >> 8) as u8, (c.b >> 8) as u8))
            .unwrap_or((0, 0, 0));
        let palette = Palette::from_bg(bg);

        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        // Resolve auto-detect on the alternate screen, so the probe glyph is
        // never visible — the first frame paints over it.
        let nerd = nerd_font.unwrap_or_else(|| probe_nerd_font().unwrap_or(false));
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self { terminal, icons: Glyphs::new(nerd), palette })
    }
}

/// Best-effort Nerd Font detection by measuring how many terminal columns a
/// Nerd Font glyph advances the cursor. A real NF glyph is drawn single-width
/// (advance 1); when the font lacks it, terminals usually substitute from an
/// emoji/symbol fallback that renders double-width (advance 2). So advance == 1
/// is our "likely present" signal.
///
/// This is a heuristic, not proof: a terminal that renders a missing glyph as a
/// single-cell box would read as a false positive. That's why it only ever sets
/// the *default* — an explicit `--nerd-font[=…]` or `TOPH_NERD_FONT` always wins,
/// and an inconclusive probe falls back to emoji (the safe, widely-rendering
/// choice). Returns `None` when stdout isn't a TTY or the terminal won't answer.
///
/// Assumes raw mode is already enabled (crossterm needs it to read the cursor
/// position report) and that we're on a scratch surface (the alternate screen).
fn probe_nerd_font() -> Option<bool> {
    if !io::stdout().is_terminal() {
        return None;
    }
    let mut out = io::stdout();
    let start = cursor::position().ok()?.0;
    write!(out, "\u{e0b0}").ok()?; // a Nerd Font powerline glyph
    out.flush().ok()?;
    let end = cursor::position().ok()?.0;
    // Wipe the probe glyph; the first real frame will repaint anyway.
    let _ = write!(out, "\r\x1b[2K");
    let _ = out.flush();
    match end.checked_sub(start) {
        Some(1) => Some(true),  // single-width → likely a real NF glyph
        Some(_) => Some(false), // double-width → emoji/symbol fallback
        None => None,           // cursor wrapped/garbage → inconclusive
    }
}

impl Renderer for TuiRenderer {
    fn present(&mut self, app: &App) -> Result<()> {
        let icons = &self.icons;
        let palette = self.palette;
        self.terminal.draw(|f| render(f, app, icons, palette))?;
        Ok(())
    }

    /// Copy via OSC 52: hand the base64-encoded text to the terminal emulator,
    /// which puts it on the real system clipboard. Works without a display
    /// server and over SSH — unlike a clipboard library — since the bytes ride
    /// the terminal we already own.
    fn copy_clipboard(&mut self, text: &str) -> Result<()> {
        let out = self.terminal.backend_mut();
        write!(out, "\x1b]52;c;{}\x07", base64_encode(text.as_bytes()))?;
        out.flush()?;
        Ok(())
    }
}

/// Standard base64 (with padding) — just enough for OSC 52, no dependency.
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[(n & 63) as usize] as char } else { '=' });
    }
    out
}

impl Drop for TuiRenderer {
    fn drop(&mut self) {
        // Best-effort restore; ignore errors during teardown (incl. panic unwind).
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod tests {
    use super::base64_encode;

    #[test]
    fn base64_matches_known_vectors() {
        // Classic RFC 4648 examples, covering both padding cases.
        assert_eq!(base64_encode(b"Man"), "TWFu"); // no padding
        assert_eq!(base64_encode(b"Ma"), "TWE="); // one pad
        assert_eq!(base64_encode(b"M"), "TQ=="); // two pads
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"/usr/bin/claude"), "L3Vzci9iaW4vY2xhdWRl");
    }
}