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
use tokio::time::{interval, MissedTickBehavior};

use crate::app::{App, Outcome};
use crate::event::map_key;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const REDRAW_INTERVAL: Duration = Duration::from_millis(500);
const CHANNEL_CAPACITY: usize = 8;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let mut terminal = setup_terminal()?;
    let res = run(&mut terminal).await;
    restore_terminal(&mut terminal)?;
    res
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
