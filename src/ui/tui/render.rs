//! The frame layout and the three regions it splits into: header, process
//! table, and status footer.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

use super::theme::{
    fade, load_color, meter_spans, Palette, ACCENT, FADE_MAX, FADE_STEP, HEADER_METER_WIDTH,
    METER_FULL_PCT, METER_WIDTH,
};
use crate::app::{App, SortBy};
use crate::ui::{icon_for, pid_cell};

pub(super) fn render(f: &mut Frame, app: &App, nerd_font: bool, palette: Palette) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_header(f, app, chunks[0], palette);
    render_table(f, app, chunks[1], nerd_font, palette);
    render_status(f, app, chunks[2]);
}

/// A reddened keyboard-shortcut character, used inline as a hint.
fn red(s: &'static str) -> Span<'static> {
    Span::styled(s, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
}

/// A label whose leading letter is its shortcut hint — e.g. `keyed("Sort")`
/// shows a red `S` then `ort`, telling the user that `s` toggles it.
fn keyed(word: &'static str) -> [Span<'static>; 2] {
    let (head, tail) = word.split_at(1);
    [red(head), Span::raw(tail)]
}

fn render_header(f: &mut Frame, app: &App, area: Rect, pal: Palette) {
    // Border first, then text in the inner area split into two columns: the
    // title + status toggles flush left, the less-important counters flush
    // right (out of the way).
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Labels are lowercase so the red mnemonic reads as the literal (lowercase)
    // key — "grouped" hints `g`, not Shift+G.
    let check = if app.grouped() { "✓" } else { " " };
    let mut status = vec![Span::raw(" hogtop     ")];
    status.extend(keyed("sort"));
    status.push(Span::raw(format!(": {}   ", app.sort_by().label())));
    status.extend(keyed("grouped"));
    status.push(Span::raw(format!(": [{check}]   ")));
    status.extend(keyed("view"));
    // Pad View to its widest value ("100") so the toggles never shift.
    status.push(Span::raw(format!(": {:<3}", app.top_n())));

    // System-wide totals, right-aligned, each with its own little gradient bar
    // like the per-process rows. CPU% and used-RAM are width-padded so the block
    // doesn't jiggle as the numbers tick.
    const GIB: f64 = (1u64 << 30) as f64;
    let total = app.total_memory().max(1) as f64;
    let cpu_frac = app.cpu_usage() as f64 / 100.0;
    let ram_frac = app.used_memory() as f64 / total;
    let mut counts = vec![Span::raw(format!("CPU {:>3.0}% ", app.cpu_usage()))];
    counts.extend(meter_spans(cpu_frac, HEADER_METER_WIDTH, 0.0, pal));
    counts.push(Span::raw(format!(
        "   ·   RAM {:>2.0}/{:.0} GB ",
        app.used_memory() as f64 / GIB,
        app.total_memory() as f64 / GIB,
    )));
    counts.extend(meter_spans(ram_frac, HEADER_METER_WIDTH, 0.0, pal));
    counts.push(Span::raw(format!("   ·   {} procs ", app.proc_count())));

    let counts_w = counts.iter().map(|s| s.content.chars().count()).sum::<usize>() as u16;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(counts_w)])
        .split(inner);

    f.render_widget(Paragraph::new(Line::from(status)), cols[0]);
    f.render_widget(Paragraph::new(Line::from(counts)).alignment(Alignment::Right), cols[1]);
}

fn render_table(f: &mut Frame, app: &App, area: Rect, nerd_font: bool, pal: Palette) {
    let ranked = app.rank_top(app.top_n());
    let selected = app.selected().min(ranked.len().saturating_sub(1));

    let rows: Vec<Row> = ranked
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mem_mb = r.avg_memory_bytes / 1024 / 1024;
            let mark = if r.is_new { "*" } else { " " };
            // The LOAD bar tracks whatever we're sorting by: CPU% over one core,
            // or memory as a share of total system RAM.
            let load_frac = match app.sort_by() {
                SortBy::Cpu => r.cpu_pct / METER_FULL_PCT,
                SortBy::Memory => r.avg_memory_bytes as f64 / app.total_memory().max(1) as f64,
            };
            let selected_row = i == selected;
            // Fade by distance from the selection (both directions). The
            // selected row stays bright; its yellow highlight owns the colors,
            // so we leave its cells unstyled and let the row style invert them.
            let dist = i.abs_diff(selected);
            let amount = (dist as f64 * FADE_STEP).min(FADE_MAX);
            let (row_style, text_style, cpu_style, meter_fade) = if selected_row {
                (
                    Style::default().fg(Color::Black).bg(Color::Yellow),
                    Style::default(),
                    Style::default(),
                    0.0,
                )
            } else {
                let cpu_t = (r.cpu_pct / METER_FULL_PCT).clamp(0.0, 1.0);
                (
                    Style::default(),
                    Style::default().fg(fade(pal.text, amount, pal.bg)),
                    Style::default().fg(fade(load_color(cpu_t), amount, pal.bg)),
                    amount,
                )
            };
            Row::new(vec![
                Cell::from(Span::styled(mark.to_string(), text_style)),
                Cell::from(Span::styled(pid_cell(r), text_style)),
                Cell::from(Line::from(meter_spans(load_frac, METER_WIDTH, meter_fade, pal))),
                Cell::from(Span::styled(format!("{:>5.1}", r.cpu_pct), cpu_style)),
                Cell::from(Span::styled(format!("{:>6} MB", mem_mb), text_style)),
                Cell::from(Span::styled(
                    format!("{} {}", icon_for(&r.label, r.platform, nerd_font), r.label),
                    text_style,
                )),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Length(8),
        Constraint::Length(METER_WIDTH),
        Constraint::Length(7),
        Constraint::Length(11),
        Constraint::Min(20),
    ];
    // The LOAD bar tracks the sorted metric, so label it to match.
    let load_header = app.sort_by().label();
    let table = Table::new(rows, widths)
        .header(
            Row::new(vec![
                Cell::from(""),
                Cell::from("PID"),
                Cell::from(load_header),
                Cell::from("CPU%"),
                Cell::from("RAM avg"),
                Cell::from("CMD"),
            ])
            .style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(ACCENT)),
        );

    let mut state = TableState::default();
    if !ranked.is_empty() {
        state.select(Some(selected));
    }
    f.render_stateful_widget(table, area, &mut state);
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let (line, style) = if let Some(pk) = app.pending_kill() {
        let target = if pk.pids.len() == 1 {
            format!("{} (PID {})", pk.label, pk.pids[0])
        } else {
            format!("{} ({} procs)", pk.label, pk.pids.len())
        };
        (
            Line::from(format!("  Send {} to {}? (y/n)", pk.signal.as_str(), target)),
            Style::default().fg(Color::Black).bg(Color::Red),
        )
    } else if let Some(msg) = app.status() {
        (Line::from(format!("  {}", msg)), Style::default().fg(Color::Cyan))
    } else {
        // Idle: the otherwise-empty footer carries the action keys, with each
        // shortcut character reddened (the toggles hint themselves in the
        // header). Base text is dimmed; the red spans override per-key.
        let mut spans = vec![
            Span::raw("  "),
            red("j"),
            Span::raw("/"),
            red("k"),
            Span::raw(" move     "),
            red("d"),
            Span::raw("/"),
            red("D"),
            Span::raw(" kill     "),
            Span::raw("cop"),
            red("y"),
            Span::raw("     "),
        ];
        spans.extend(keyed("reset"));
        spans.push(Span::raw("     "));
        spans.extend(keyed("quit"));
        (Line::from(spans), Style::default().fg(Color::DarkGray))
    };
    f.render_widget(Paragraph::new(line).style(style), area);
}