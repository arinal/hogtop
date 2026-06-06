use std::io::Write;

use anyhow::Result;

use super::{icon_for, pid_cell, Renderer};
use crate::app::App;

/// Snapshot interpreter: writes the plain-text table to any `Write` sink.
pub struct PlainRenderer<W: Write> {
    sink: W,
    top: usize,
    nerd_font: bool,
}

impl<W: Write> PlainRenderer<W> {
    pub fn new(sink: W, top: usize, nerd_font: bool) -> Self {
        Self { sink, top, nerd_font }
    }
}

impl<W: Write> Renderer for PlainRenderer<W> {
    fn present(&mut self, app: &App) -> Result<()> {
        write!(self.sink, "{}", render_plain(app, self.top, self.nerd_font))?;
        Ok(())
    }
}

/// Renders the ranked table as plain text (no TTY) for snapshot/`--once` mode.
fn render_plain(app: &App, top: usize, nerd_font: bool) -> String {
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
        out.push_str(&format!(
            "{mark}{:>6}  {:>6.1}  {:>6} MB  {} {}\n",
            pid_cell(&r),
            r.cpu_pct,
            mem_mb,
            icon_for(&r.label, r.platform, nerd_font),
            r.label,
        ));
    }
    out
}