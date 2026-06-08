//! Theme-aware colors and the gradient meter bar. Everything here blends toward
//! the terminal's real background so the distance fade works in light or dark.

use ratatui::{
    style::{Color, Style},
    text::Span,
};

/// btop's box-accent color: cyan borders throughout.
pub(super) const ACCENT: Color = Color::Cyan;
/// Width (in cells) of the per-row CPU meter bar.
pub(super) const METER_WIDTH: u16 = 16;
/// Width of the smaller CPU/RAM total bars in the header.
pub(super) const HEADER_METER_WIDTH: u16 = 10;
/// The meter is full at this CPU% (one fully-loaded core), so a busy single
/// thread fills the bar without needing all cores pegged.
pub(super) const METER_FULL_PCT: f64 = 100.0;
/// The filled and empty meter cell glyph (btop uses `■`).
const METER_CELL: char = '■';
/// btop-style distance fade: a row dims by this much per step away from the
/// selection (in either direction)…
pub(super) const FADE_STEP: f64 = 0.06;
/// …capped here so even distant rows stay legible.
pub(super) const FADE_MAX: f64 = 0.66;

/// A light/dark-aware color set derived from the terminal's real background, so
/// the distance fade blends toward whatever the user is actually running. Text
/// gets an explicit base color (a terminal's default foreground can't be
/// blended toward the background), chosen to contrast with the detected theme.
#[derive(Clone, Copy)]
pub(super) struct Palette {
    /// The detected terminal background — the color every row fades toward.
    pub bg: (u8, u8, u8),
    /// Base foreground for ordinary row text before fading.
    pub text: Color,
    /// Empty (unfilled) meter cells: a low-contrast tint of the background.
    pub meter_empty: Color,
}

impl Palette {
    /// Derive the palette from the terminal background `bg`. Uses Rec. 601 luma
    /// to decide light vs. dark: a light background needs dark text (and a dark
    /// background light text) so rows stay legible until they fade away.
    pub fn from_bg(bg: (u8, u8, u8)) -> Self {
        let luma = 0.299 * bg.0 as f64 + 0.587 * bg.1 as f64 + 0.114 * bg.2 as f64;
        if luma > 128.0 {
            Self {
                bg,
                text: Color::Rgb(0x30, 0x30, 0x30),
                meter_empty: Color::Rgb(0xc8, 0xc8, 0xc8),
            }
        } else {
            Self {
                bg,
                text: Color::Rgb(0xcc, 0xcc, 0xcc),
                meter_empty: Color::Rgb(0x3c, 0x3c, 0x3c),
            }
        }
    }
}

/// Blend `color` toward the terminal background `bg` by `amount` (0.0..=1.0):
/// 0.0 leaves it untouched, 1.0 fades fully into the background. This drives the
/// fade — rows farther from the selection dim away regardless of theme. Only
/// RGB colors fade; named colors pass through unchanged.
pub(super) fn fade(color: Color, amount: f64, bg: (u8, u8, u8)) -> Color {
    let f = amount.clamp(0.0, 1.0);
    if let Color::Rgb(r, g, b) = color {
        let lerp = |c: u8, t: u8| (c as f64 + (t as f64 - c as f64) * f).round() as u8;
        Color::Rgb(lerp(r, bg.0), lerp(g, bg.1), lerp(b, bg.2))
    } else {
        color
    }
}

/// btop's load gradient: green → yellow → red, interpolated in two linear
/// segments (0–50 green→yellow, 50–100 yellow→red), mirroring btop's
/// `generateGradients` three-stop lerp. `t` is clamped to 0.0..=1.0.
pub(super) fn load_color(t: f64) -> Color {
    const GREEN: (u8, u8, u8) = (0x69, 0xff, 0x94);
    const YELLOW: (u8, u8, u8) = (0xff, 0xe0, 0x66);
    const RED: (u8, u8, u8) = (0xff, 0x55, 0x55);
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: u8, b: u8, f: f64| (a as f64 + (b as f64 - a as f64) * f).round() as u8;
    let ((r0, g0, b0), (r1, g1, b1), f) = if t < 0.5 {
        (GREEN, YELLOW, t / 0.5)
    } else {
        (YELLOW, RED, (t - 0.5) / 0.5)
    };
    Color::Rgb(lerp(r0, r1, f), lerp(g0, g1, f), lerp(b0, b1, f))
}

/// A `width`-cell gradient meter bar for a 0.0..=1.0 load `frac`: each filled
/// cell colored by its own position along the load gradient (so the bar shades
/// green→red as it grows), empty cells dimmed. Mirrors btop's `Meter`.
/// `fade_amt` dims the whole bar toward the background for the distance fade.
pub(super) fn meter_spans(frac: f64, width: u16, fade_amt: f64, pal: Palette) -> Vec<Span<'static>> {
    let frac = frac.clamp(0.0, 1.0);
    let filled = (frac * width as f64).round() as u16;
    (0..width)
        .map(|i| {
            let color = if i < filled {
                // Color each cell by its own height along the bar, like btop.
                let cell_t = (i + 1) as f64 / width as f64;
                load_color(cell_t)
            } else {
                pal.meter_empty
            };
            Span::styled(
                METER_CELL.to_string(),
                Style::default().fg(fade(color, fade_amt, pal.bg)),
            )
        })
        .collect()
}