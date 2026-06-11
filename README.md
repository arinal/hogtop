<h1>
  <img src="https://raw.githubusercontent.com/arinal/toph/main/assets/icon-256.png" width="38" alt="" valign="middle">
  toph
</h1>

> **toph** - *top hog*. An anagram of `htop`, named after the blind earthbender
> who senses what's hidden through vibrations, then strikes with precision.
> First you examine the CPU hog; then you deal with it.

An interactive TUI that ranks the most CPU- (or memory-) hungry processes over a
**growing window**, groups multi-process apps into one row, and lets you kill
them gracefully or forcefully.

![toph in action](https://raw.githubusercontent.com/arinal/toph/main/assets/screenshot.png)

Unlike `htop` (which shows instantaneous CPU usage, causing the list to jitter
constantly) or `ps --sort=-%cpu` (which reports the *lifetime* average since
each process started), toph averages CPU usage over the wall-clock time
*since you launched it*. The ranking gets more stable the longer you watch.

## Why

Built to scratch a specific itch: "something is eating my CPU right now,
which process is it, and let me kill it without having to play whack-a-mole
with a constantly-reordering top window."

- **Growing window**: the average stabilizes as more time passes — you can
  act as soon as you trust the ranking.
- **App grouping**: Chrome's 40 renderers or an Electron app's helper swarm
  collapse into a single row with summed CPU/RAM, so you see the app, not the
  process soup. One kill takes the whole group down.
- **Recognizable rows**: processes are classified into runtime families
  (Chrome, Firefox, Electron, Java, Python, Node, shells, kernel threads) and
  known apps, each shown with an icon and a friendly label like
  `Chrome — renderer` instead of a raw command line.
- **Confirmed kill**: no accidental SIGKILLs from a fat-finger.
- **Non-blocking UI**: sampling runs on a separate task; key input never
  lags while the process table is being read.

## Install / Run

### From crates.io

```sh
cargo install toph
```

### From source

```sh
cargo install --git https://github.com/arinal/toph    # latest from main
# or, from a local checkout:
cargo install --path .                                # drops `toph` in ~/.cargo/bin
cargo run --release                                   # run without installing
```

The interactive TUI requires a real TTY (raw mode), so it won't work piped or
with redirected stdin.

### Icons

By default toph uses emoji glyphs, which render in most modern terminals. If
you have a [Nerd Font](https://www.nerdfonts.com/) installed, pass `--nerd-font`
for sharper, monospace-aligned icons:

```sh
toph --nerd-font
```

### Snapshot mode (non-interactive)

For scripts, CI, or piping - sample for a few seconds and print a plain table:

```sh
toph --once               # 3s window, default rows
toph --once --secs 5      # 5s window
toph --once --top 50      # 50 rows
```

No TTY needed; output goes to stdout. Rows that appeared *after* the window
started are marked with a leading `*`.

## Keybinds

| Key                    | Action                                                                |
|------------------------|-----------------------------------------------------------------------|
| `j` / `k` or `↓` / `↑` | Move selection (vim-style or arrows)                                  |
| `s`                    | Toggle sort: CPU ⇄ RAM                                                 |
| `v`                    | Cycle view size: 25 → 50 → 100 → 25…                                  |
| `g`                    | Toggle app grouping (on by default). Grouped: Chromium browsers, Firefox, and Electron apps collapse to one row per app, CPU/RAM summed. Ungrouped: one row per process (per-tab / per-renderer). |
| `y`                    | Copy the selected row's executable path to the clipboard (OSC 52)     |
| `d`                    | Send SIGTERM (graceful) to the selection - confirms with `y/n`        |
| `D`                    | Send SIGKILL (force) to the selection - confirms with `y/n`           |
| `r`                    | Reset the averaging window - re-baselines all processes               |
| `q` / `Esc` / `Ctrl-C` | Quit                                                                  |

Killing a grouped row signals every process in the group. Copy uses OSC 52, so
the yanked path lands on your real clipboard even over SSH.

## How the averaging works

Every second, the sampler refreshes the process table (via the `sysinfo` crate)
and records each process's total accumulated CPU time and current resident
memory.

**CPU%** derived from a cumulative counter, so we diff over the window:

```
cpu% = (cpu_ms_now - cpu_ms_at_baseline)
     / (wall_ms_now - wall_ms_at_baseline)
     / num_cores
     * 100
```

**RAM avg** — memory is a gauge, not a counter, so we accumulate samples
and report the arithmetic mean of observed RSS over the window:

```
ram_avg = sum(rss_samples) / sample_count
```

The CPU baseline and memory accumulator are captured the first time a
process is observed (typically at launch). Pressing `r` re-baselines and
resets the memory accumulator for every process. A process that exits is
dropped from the table; a process that starts mid-run is inserted with
fresh state. A row needs at least half a second of window before its CPU%
is considered meaningful, so brand-new processes briefly sit out of the
ranking.

## Architecture

The code is structured as a small "onion": a pure core (domain state and
logic) surrounded by side-effecting adapters (terminal, signals, sampling),
wired together in `main.rs`. The boundaries are expressed as traits (ports)
so the core can be tested against fakes.

| Module          | Role                                                                 |
|-----------------|----------------------------------------------------------------------|
| `main.rs`       | Terminal setup, tokio runtime, the `select!` event loop, CLI args    |
| `app.rs`        | `App` state, ranking, grouping, `Action` → state transitions         |
| `sampler.rs`    | Refreshes `sysinfo`, classifies processes, emits `Snapshot`s         |
| `control.rs`    | `ProcessController` port + the `nix`-backed signal adapter           |
| `classifier/`   | Per-family detectors (Chrome, Firefox, Electron, Java, Python, …) that turn argv into a runtime `Platform`, a friendly label, and an app-group identity |
| `ui/`           | `Renderer` + `IconSet` ports, the interactive `tui/` renderer, the `plain` snapshot renderer, and the icon glyph table |

Sampling runs on a `tokio` task because `sysinfo`'s process refresh can take
50–200ms on busy systems — running it on the main loop would freeze key input.
Snapshots flow over an `mpsc` channel; the UI task awaits them in
`tokio::select!` alongside the crossterm `EventStream` and a redraw timer.

Process control and clipboard access are behind traits, so the core's
selection, pid, and signal logic is unit-tested with a recording fake. No real
processes are signalled in the test suite.

## Not implemented

- No CSV/JSON export. If you want to script over the data, `top -b -n 1`
  or `ps` is probably what you want anyway.
- No filtering by user, command, or cgroup. Easy to add — open an issue or
  hack `App::rank_top`.
- No remote / SSH process control. Run it on the host you care about.
- Linux/macOS in practice. `sysinfo` is cross-platform, but
  `nix::sys::signal::kill` ties us to Unix. Windows support would need a small
  `#[cfg]` swap.

## License

Licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.
