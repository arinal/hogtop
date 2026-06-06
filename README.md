# hogtop

A small interactive TUI that ranks the top 10 CPU-hogging processes over a
**growing window** and lets you kill them gracefully or forcefully.

Unlike `htop` (which shows instantaneous CPU usage, causing the list to jitter
constantly) or `ps --sort=-%cpu` (which reports the *lifetime* average since
each process started), hogtop averages CPU usage over the wall-clock time
*since you launched it*. The ranking gets more stable the longer you watch.

## Why

Built to scratch a specific itch: "something is eating my CPU right now,
which process is it, and let me kill it without having to play whack-a-mole
with a constantly-reordering top window."

- **Growing window**: the average stabilizes as more time passes — you can
  act as soon as you trust the ranking.
- **Confirmed kill**: no accidental SIGKILLs from a fat-finger.
- **Non-blocking UI**: sampling runs on a separate task; key input never
  lags while `/proc` is being read.

## Install / Run

```sh
cargo run --release
```

A `cargo install --path .` will drop a `hogtop` binary in `~/.cargo/bin/`.

The interactive TUI requires a real TTY (raw mode), so it won't work piped or
with redirected stdin.

### Snapshot mode (non-interactive)

For scripts, CI, or piping — sample for a few seconds and print a plain table:

```sh
hogtop --once               # 3s window, top 10
hogtop --once --secs 5      # 5s window
hogtop --once --top 25      # top 25 rows
```

No TTY needed; output goes to stdout.

## Keybinds

| Key                    | Action                                                                |
|------------------------|-----------------------------------------------------------------------|
| `j` / `k` or `↓` / `↑` | Move selection (vim-style or arrows)                                  |
| `c`                    | Sort by CPU (default)                                                 |
| `m`                    | Sort by average memory                                                |
| `v`                    | Cycle view size: 10 → 50 → 100 → 10…                                 |
| `g`                    | Toggle grouping of multi-process apps — Chromium browsers (Chrome/Chromium) and Electron apps (on by default): one row per app, CPU/memory summed across its processes. Off = one row per process (per-tab/per-renderer). |
| `d`                    | Send SIGTERM (graceful) to the selected process — confirms with `y/n` |
| `D`                    | Send SIGKILL (force) to the selected process — confirms with `y/n`    |
| `r`                    | Reset the averaging window — re-baselines all processes               |
| `q` / `Esc` / `Ctrl-C` | Quit                                                                  |

Processes marked with `*` appeared *after* the window started, so their
percentage is computed over a shorter sub-window — treat with mild
skepticism until you've watched them for a few seconds.

## How the averaging works

Every second, the sampler refreshes `/proc/<pid>/stat` (via the `sysinfo`
crate) and records each process's total accumulated CPU time and current
resident memory.

**CPU%** — derived from a cumulative counter, so we diff over the window:

```
cpu% = (cpu_ms_now - cpu_ms_at_baseline)
     / (wall_ms_now - wall_ms_at_baseline)
     / num_cores
     * 100
```

**MEM avg** — memory is a gauge, not a counter, so we accumulate samples
and report the arithmetic mean of observed RSS over the window:

```
mem_avg = sum(rss_samples) / sample_count
```

The CPU baseline and memory accumulator are captured the first time a
process is observed (typically at launch). Pressing `r` re-baselines and
resets the memory accumulator for every process. A process that exits is
dropped from the table; a process that starts mid-run is inserted with
fresh state and marked `*`.

## Architecture

Five small modules, ~400 LOC total:

| Module       | Role                                                          |
|--------------|---------------------------------------------------------------|
| `main.rs`    | Terminal setup, tokio runtime, the `select!` event loop       |
| `app.rs`     | `App` state, ranking, action handling (kill / reset / nav)    |
| `sampler.rs` | Blocking task that refreshes sysinfo and sends `Snapshot`s    |
| `event.rs`   | Maps crossterm `KeyEvent`s into a typed `Action` enum         |
| `ui.rs`      | Pure render functions for the header, table, and status line  |

Sampling is wrapped in `tokio::task::spawn_blocking` because
`sysinfo::System::refresh_processes` can take 50–200ms on busy systems —
running it on the main loop would freeze key input. Snapshots flow over an
`mpsc` channel; the UI task awaits them in `tokio::select!` alongside the
crossterm `EventStream` and a redraw timer.

## Not implemented

- No CSV/JSON export. If you want to script over the data, `top -b -n 1`
  or `ps` is probably what you want anyway.
- No filtering by user, command, or cgroup. Easy to add — open an issue or
  hack `App::rank_top`.
- No remote / SSH process control. Run it on the host you care about.
- Linux-only in practice. `sysinfo` is cross-platform, but `nix::sys::signal::kill`
  ties us to Unix. Windows support would need a small `#[cfg]` swap.