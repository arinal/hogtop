mod app;
mod classifier;
mod control;
mod sampler;
mod ui;

use std::io;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use crossterm::event::{Event as CtEvent, EventStream};
use futures_util::StreamExt;
use tokio::time::{MissedTickBehavior, interval};

use crate::app::{App, Outcome, VIEW_SIZES};
use crate::control::ProcessController;
use crate::ui::{PlainRenderer, Renderer, TuiRenderer, map_key};

const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const REDRAW_INTERVAL: Duration = Duration::from_millis(500);

/// Interactive process monitor; pass --once for a one-shot plain table.
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Non-interactive: sample for a few seconds, print a plain table, exit.
    #[arg(long, visible_alias = "snapshot")]
    once: bool,
    /// How long to sample before printing, in snapshot mode.
    #[arg(long, default_value_t = 3)]
    secs: u64,
    /// How many rows to print.
    #[arg(long, default_value_t = VIEW_SIZES[0])]
    top: usize,
    /// Use Nerd Font glyphs for icons instead of emoji (requires a Nerd Font).
    #[arg(long)]
    nerd_font: bool,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.once {
        run_snapshot(args.secs, args.top, args.nerd_font)
    } else {
        let nerd_font = args.nerd_font;
        let mut renderer = TuiRenderer::new(nerd_font)?;
        run(&mut renderer, control::Nix).await
    }
}

/// Headless mode: sample synchronously over `args.secs`, then present one plain
/// table. No background thread, raw mode, or alternate screen — nothing runs
/// concurrently, so it works without a TTY and can be piped. CPU% needs >= 2
/// samples, so the window is at least 2 ticks.
fn run_snapshot(secs: u64, top: usize, nerd_font: bool) -> Result<()> {
    let app = App::sampled(secs, SAMPLE_INTERVAL);
    PlainRenderer::new(io::stdout(), top, nerd_font).present(&app)
}

async fn run<R: Renderer, C: ProcessController>(renderer: &mut R, ctrl: C) -> Result<()> {
    let mut app = App::new();
    let mut snapshot_rx = sampler::Sampler::new().spawn(SAMPLE_INTERVAL);

    let mut events = EventStream::new();
    let mut redraw = interval(REDRAW_INTERVAL);
    redraw.set_missed_tick_behavior(MissedTickBehavior::Skip);

    renderer.present(&app)?;

    loop {
        tokio::select! {
            Some(snapshot) = snapshot_rx.recv() => {
                app.ingest(snapshot);
            }
            Some(Ok(CtEvent::Key(key))) = events.next() => {
                if let Some(action) = map_key(key, app.has_pending_kill()) {
                    match app.apply(action, &ctrl) {
                        Outcome::Quit => return Ok(()),
                        Outcome::Copy(text) => renderer.copy_clipboard(&text)?,
                        Outcome::Continue => {}
                    }
                }
            }
            _ = redraw.tick() => {}
        }
        app.expire_status();
        renderer.present(&app)?;
    }
}
