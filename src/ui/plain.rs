use std::io::Write;

use anyhow::Result;

use super::{badges, grouped_badge, pid_cell, Glyphs, IconSet, Renderer};
use crate::app::App;

/// Snapshot interpreter: writes the plain-text table to any `Write` sink.
pub struct PlainRenderer<W: Write> {
    sink: W,
    top: usize,
    icons: Glyphs,
}

impl<W: Write> PlainRenderer<W> {
    pub fn new(sink: W, top: usize, nerd_font: bool) -> Self {
        Self { sink, top, icons: Glyphs::new(nerd_font) }
    }
}

impl<W: Write> Renderer for PlainRenderer<W> {
    fn present(&mut self, app: &App) -> Result<()> {
        write!(self.sink, "{}", render_plain(app, self.top, &self.icons))?;
        Ok(())
    }
}

/// Renders the ranked table as plain text (no TTY) for snapshot/`--once` mode.
fn render_plain(app: &App, top: usize, icons: &dyn IconSet) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "hogtop — window: {}s · {} procs · sort: {}\n",
        app.window_elapsed().as_secs(),
        app.proc_count(),
        app.sort_by().label(),
    ));
    out.push_str(&format!(
        "{:>7}  {:>6}  {:>9}  CMD\n",
        "PID", "CPU%", "MEM avg"
    ));
    for r in app.rank_top(top) {
        let mem_mb = r.avg_memory_bytes / 1024 / 1024;
        let mark = if r.is_new { '*' } else { ' ' };
        // Platform/identity badges, then the grouped `N×` count — plain text
        // can't tint it, so it just trails the other badges.
        let mut tags = badges(&r, icons);
        tags.extend(grouped_badge(&r));
        let mut prefix = tags.join(" ");
        if !prefix.is_empty() {
            prefix.push(' ');
        }
        out.push_str(&format!(
            "{mark}{:>6}  {:>6.1}  {:>6} MB  {}{}\n",
            pid_cell(&r),
            r.cpu_pct,
            mem_mb,
            prefix,
            r.label,
        ));
    }
    out
}