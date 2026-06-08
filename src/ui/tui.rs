use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::{
    event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use nix::sys::signal::Signal;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame, Terminal,
};

use super::{icon_for, pid_cell, Renderer};
use crate::app::{Action, App, SortBy};

type TuiBackend = CrosstermBackend<Stdout>;

/// btop's box-accent color: cyan borders throughout.
const ACCENT: Color = Color::Cyan;
/// Width (in cells) of the per-row CPU meter bar.
const METER_WIDTH: u16 = 16;
/// The meter is full at this CPU% (one fully-loaded core), so a busy single
/// thread fills the bar without needing all cores pegged.
const METER_FULL_PCT: f64 = 100.0;
/// The filled and empty meter cell glyph (btop uses `■`).
const METER_CELL: char = '■';

/// btop's load gradient: green → yellow → red, interpolated in two linear
/// segments (0–50 green→yellow, 50–100 yellow→red), mirroring btop's
/// `generateGradients` three-stop lerp. `t` is clamped to 0.0..=1.0.
fn load_color(t: f64) -> Color {
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

/// A gradient meter bar for `cpu_pct`: `METER_WIDTH` cells, each filled cell
/// colored by its own position along the load gradient (so the bar shades
/// green→red as it grows), empty cells dimmed. Mirrors btop's `Meter`.
fn meter_spans(cpu_pct: f64) -> Vec<Span<'static>> {
    let frac = (cpu_pct / METER_FULL_PCT).clamp(0.0, 1.0);
    let filled = (frac * METER_WIDTH as f64).round() as u16;
    (0..METER_WIDTH)
        .map(|i| {
            if i < filled {
                // Color each cell by its own height along the bar, like btop.
                let cell_t = (i + 1) as f64 / METER_WIDTH as f64;
                Span::styled(
                    METER_CELL.to_string(),
                    Style::default().fg(load_color(cell_t)),
                )
            } else {
                Span::styled(
                    METER_CELL.to_string(),
                    Style::default().fg(Color::DarkGray),
                )
            }
        })
        .collect()
}

/// Frontend input mapping: translate a crossterm key event into a core
/// [`Action`]. This is the UI's concern — the core never sees a keystroke.
pub fn map_key(k: KeyEvent, kill_pending: bool) -> Option<Action> {
    if k.kind != KeyEventKind::Press {
        return None;
    }
    if kill_pending {
        return Some(match k.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => Action::ConfirmKill,
            _ => Action::CancelKill,
        });
    }
    match k.code {
        KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
        KeyCode::Char('r') => Some(Action::Reset),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::SelectNext),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::SelectPrev),
        KeyCode::Char('d') => Some(Action::RequestKill(Signal::SIGTERM)),
        KeyCode::Char('D') => Some(Action::RequestKill(Signal::SIGKILL)),
        KeyCode::Char('c') => Some(Action::SortBy(SortBy::Cpu)),
        KeyCode::Char('m') => Some(Action::SortBy(SortBy::Memory)),
        KeyCode::Char('v') => Some(Action::CycleViewSize),
        KeyCode::Char('g') => Some(Action::ToggleGroup),
        _ => None,
    }
}

/// Interactive interpreter: owns the terminal and its raw-mode/alternate-screen
/// lifecycle. `new` sets it up; `Drop` restores it — so callers never touch
/// terminal plumbing.
pub struct TuiRenderer {
    terminal: Terminal<TuiBackend>,
    nerd_font: bool,
}

impl TuiRenderer {
    pub fn new(nerd_font: bool) -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self { terminal, nerd_font })
    }
}

impl Renderer for TuiRenderer {
    fn present(&mut self, app: &App) -> Result<()> {
        let nerd_font = self.nerd_font;
        self.terminal.draw(|f| render(f, app, nerd_font))?;
        Ok(())
    }
}

impl Drop for TuiRenderer {
    fn drop(&mut self) {
        // Best-effort restore; ignore errors during teardown (incl. panic unwind).
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn render(f: &mut Frame, app: &App, nerd_font: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_header(f, app, chunks[0]);
    render_table(f, app, chunks[1], nerd_font);
    render_status(f, app, chunks[2]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let text = format!(
        " hogtop  |  window: {}s  |  {} procs  |  sort: {}  |  view: {}  |  group: {}  |  j/k  c/m sort  v view  g group  d/D kill  r reset  q quit",
        app.window_elapsed().as_secs(),
        app.proc_count(),
        app.sort_by().label(),
        app.top_n(),
        if app.grouped() { "on" } else { "off" },
    );
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(ACCENT)),
        ),
        area,
    );
}

fn render_table(f: &mut Frame, app: &App, area: Rect, nerd_font: bool) {
    let ranked = app.rank_top(app.top_n());
    let selected = app.selected().min(ranked.len().saturating_sub(1));

    let rows: Vec<Row> = ranked
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mem_mb = r.avg_memory_bytes / 1024 / 1024;
            let mark = if r.is_new { "*" } else { " " };
            let selected_row = i == selected;
            let style = if selected_row {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else {
                Style::default()
            };
            // CPU% colored by load, like btop — but on the selected row the
            // yellow highlight owns the colors, so leave the text to invert.
            let cpu_style = if selected_row {
                Style::default()
            } else {
                Style::default().fg(load_color((r.cpu_pct / METER_FULL_PCT).clamp(0.0, 1.0)))
            };
            Row::new(vec![
                Cell::from(mark.to_string()),
                Cell::from(pid_cell(r)),
                Cell::from(Span::styled(format!("{:>5.1}", r.cpu_pct), cpu_style)),
                Cell::from(Line::from(meter_spans(r.cpu_pct))),
                Cell::from(format!("{:>6} MB", mem_mb)),
                Cell::from(format!("{} {}", icon_for(&r.label, r.platform, nerd_font), r.label)),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Length(8),
        Constraint::Length(7),
        Constraint::Length(METER_WIDTH),
        Constraint::Length(11),
        Constraint::Min(20),
    ];
    let table = Table::new(rows, widths)
        .header(
            Row::new(vec!["", "PID", "CPU%", "LOAD", "MEM avg", "CMD"])
                .style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(ACCENT))
                .title(" Top Hogs (* = appeared after window start) "),
        );

    let mut state = TableState::default();
    if !ranked.is_empty() {
        state.select(Some(selected));
    }
    f.render_stateful_widget(table, area, &mut state);
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let (text, style) = if let Some(pk) = app.pending_kill() {
        let target = if pk.pids.len() == 1 {
            format!("{} (PID {})", pk.label, pk.pids[0])
        } else {
            format!("{} ({} procs)", pk.label, pk.pids.len())
        };
        (
            format!("  Send {} to {}? (y/n)", pk.signal.as_str(), target),
            Style::default().fg(Color::Black).bg(Color::Red),
        )
    } else if let Some(msg) = app.status() {
        (format!("  {}", msg), Style::default().fg(Color::Cyan))
    } else {
        (String::new(), Style::default())
    };
    f.render_widget(Paragraph::new(text).style(style), area);
}