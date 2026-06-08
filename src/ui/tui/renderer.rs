//! The interactive interpreter: owns the terminal and its raw-mode/alternate-
//! screen lifecycle, and drives a frame per `present`.

use std::io::{self, Stdout, Write};
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use super::render::render;
use super::theme::Palette;
use crate::app::App;
use crate::ui::Renderer;

type TuiBackend = CrosstermBackend<Stdout>;

/// Interactive interpreter: owns the terminal and its raw-mode/alternate-screen
/// lifecycle. `new` sets it up; `Drop` restores it — so callers never touch
/// terminal plumbing.
pub struct TuiRenderer {
    terminal: Terminal<TuiBackend>,
    nerd_font: bool,
    palette: Palette,
}

impl TuiRenderer {
    pub fn new(nerd_font: bool) -> Result<Self> {
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
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self { terminal, nerd_font, palette })
    }
}

impl Renderer for TuiRenderer {
    fn present(&mut self, app: &App) -> Result<()> {
        let nerd_font = self.nerd_font;
        let palette = self.palette;
        self.terminal.draw(|f| render(f, app, nerd_font, palette))?;
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