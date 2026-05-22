use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

use crate::app::App;

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_header(f, app, chunks[0]);
    render_table(f, app, chunks[1]);
    render_status(f, app, chunks[2]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let text = format!(
        " hogtop  |  window: {}s  |  {} procs  |  sort: {}  |  view: {}  |  j/k select   c/m sort   v view   d/D kill   r reset   q quit",
        app.window_elapsed().as_secs(),
        app.proc_count(),
        app.sort_by().label(),
        app.top_n(),
    );
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn render_table(f: &mut Frame, app: &App, area: Rect) {
    let ranked = app.rank_top(app.top_n());
    let selected = app.selected().min(ranked.len().saturating_sub(1));

    let rows: Vec<Row> = ranked
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mem_mb = r.avg_memory_bytes / 1024 / 1024;
            let mark = if r.state.is_new { "*" } else { " " };
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(mark.to_string()),
                Cell::from(r.pid.to_string()),
                Cell::from(format!("{:>5.1}", r.cpu_pct)),
                Cell::from(format!("{:>6} MB", mem_mb)),
                Cell::from(r.state.cmd.clone()),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Length(8),
        Constraint::Length(7),
        Constraint::Length(11),
        Constraint::Min(20),
    ];
    let table = Table::new(rows, widths)
        .header(
            Row::new(vec!["", "PID", "CPU%", "MEM avg", "CMD"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
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
        (
            format!("  Send {} to PID {}? (y/n)", pk.signal.as_str(), pk.pid),
            Style::default().fg(Color::Black).bg(Color::Red),
        )
    } else if let Some(msg) = app.status() {
        (format!("  {}", msg), Style::default().fg(Color::Cyan))
    } else {
        (String::new(), Style::default())
    };
    f.render_widget(Paragraph::new(text).style(style), area);
}
