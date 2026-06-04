mod app;
mod event;
mod identify;
mod sampler;
mod ui;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{Event as CtEvent, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;
use tokio::time::{interval, timeout, MissedTickBehavior};

use crate::app::{App, Outcome, VIEW_SIZES};
use crate::event::map_key;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const REDRAW_INTERVAL: Duration = Duration::from_millis(500);
const CHANNEL_CAPACITY: usize = 8;

/// Parsed command-line options.
struct Args {
    /// Non-interactive: sample for a few seconds, print a plain table, exit.
    snapshot: bool,
    /// How long to sample before printing, in snapshot mode.
    secs: u64,
    /// How many rows to print.
    top: usize,
}

impl Args {
    fn parse() -> Self {
        let mut args = Args {
            snapshot: false,
            secs: 3,
            top: VIEW_SIZES[0],
        };
        let mut it = std::env::args().skip(1);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--once" | "--snapshot" => args.snapshot = true,
                "--secs" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        args.secs = v;
                    }
                }
                "--top" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        args.top = v;
                    }
                }
                _ => {}
            }
        }
        args
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.snapshot {
        return run_snapshot(args).await;
    }
    let mut terminal = setup_terminal()?;
    let res = run(&mut terminal).await;
    restore_terminal(&mut terminal)?;
    res
}

/// Headless mode: collect samples for `args.secs`, then print one plain table.
/// No raw mode / alternate screen, so it works without a TTY and can be piped.
async fn run_snapshot(args: Args) -> Result<()> {
    let mut app = App::new();
    let (snapshot_tx, mut snapshot_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let _sampler = sampler::spawn(snapshot_tx, SAMPLE_INTERVAL);

    // Ingest snapshots until the window elapses; CPU% needs >= 2 samples.
    let _ = timeout(Duration::from_secs(args.secs.max(2)), async {
        while let Some(snapshot) = snapshot_rx.recv().await {
            app.ingest(snapshot);
        }
    })
    .await;

    print!("{}", ui::render_plain(&app, args.top));
    Ok(())
}

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

fn setup_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

async fn run(terminal: &mut Tui) -> Result<()> {
    let mut app = App::new();
    let (snapshot_tx, mut snapshot_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let _sampler = sampler::spawn(snapshot_tx, SAMPLE_INTERVAL);

    let mut events = EventStream::new();
    let mut redraw = interval(REDRAW_INTERVAL);
    redraw.set_missed_tick_behavior(MissedTickBehavior::Skip);

    terminal.draw(|f| ui::render(f, &app))?;

    loop {
        tokio::select! {
            Some(snapshot) = snapshot_rx.recv() => {
                app.ingest(snapshot);
            }
            Some(Ok(CtEvent::Key(key))) = events.next() => {
                if let Some(action) = map_key(key, app.has_pending_kill())
                    && let Outcome::Quit = app.apply(action)
                {
                    return Ok(());
                }
            }
            _ = redraw.tick() => {}
        }
        app.expire_status();
        terminal.draw(|f| ui::render(f, &app))?;
    }
}
