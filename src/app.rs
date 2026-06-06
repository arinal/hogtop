use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use nix::sys::signal::Signal;
use sysinfo::Pid;

use crate::classifier::Platform;
use crate::control::ProcessController;
use crate::sampler::{Sampler, Snapshot};

pub const VIEW_SIZES: [usize; 3] = [10, 50, 100];
const STATUS_TTL: Duration = Duration::from_secs(3);
const MIN_WINDOW_SECS: f64 = 0.5;

pub struct ProcState {
    pub cmd: String,
    pub group: Option<String>,
    pub platform: Platform,
    pub baseline_cpu_ms: u64,
    pub baseline_at: Instant,
    pub last_cpu_ms: u64,
    pub last_seen: Instant,
    pub last_memory_bytes: u64,
    pub memory_sum_bytes: u128,
    pub memory_samples: u32,
    pub is_new: bool,
}

impl ProcState {
    pub fn avg_memory_bytes(&self) -> u64 {
        if self.memory_samples == 0 {
            self.last_memory_bytes
        } else {
            (self.memory_sum_bytes / self.memory_samples as u128) as u64
        }
    }

    /// Windowed CPU% over `num_cores`, or `None` if the sample window is still
    /// too short for the figure to be meaningful.
    pub fn cpu_pct(&self, num_cores: f64) -> Option<f64> {
        let elapsed = self
            .last_seen
            .saturating_duration_since(self.baseline_at)
            .as_secs_f64();
        if elapsed < MIN_WINDOW_SECS {
            return None;
        }
        let cpu_secs = self.last_cpu_ms.saturating_sub(self.baseline_cpu_ms) as f64 / 1000.0;
        Some(cpu_secs / elapsed / num_cores * 100.0)
    }
}

/// A ranked table row: either a single process or an aggregated Electron-app
/// group (multiple PIDs, summed CPU/memory).
pub struct Row {
    pub label: String,
    pub cpu_pct: f64,
    pub avg_memory_bytes: u64,
    pub pids: Vec<Pid>,
    pub is_group: bool,
    pub is_new: bool,
    pub platform: Platform,
}

#[derive(Debug, Clone)]
pub struct PendingKill {
    pub pids: Vec<Pid>,
    pub signal: Signal,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Cpu,
    Memory,
}

/// The core's input vocabulary — every state transition `apply` can make.
/// This is the "message" type of the update loop; how raw input maps to it is
/// the frontend's concern (see `event::map_key`).
#[derive(Debug, Clone, Copy)]
pub enum Action {
    Quit,
    Reset,
    SelectNext,
    SelectPrev,
    RequestKill(Signal),
    ConfirmKill,
    CancelKill,
    SortBy(SortBy),
    CycleViewSize,
    ToggleGroup,
}

impl SortBy {
    pub fn label(self) -> &'static str {
        match self {
            SortBy::Cpu => "cpu",
            SortBy::Memory => "mem",
        }
    }
}

pub struct App {
    procs: HashMap<Pid, ProcState>,
    window_start: Instant,
    first_snapshot: bool,
    num_cores: f64,
    selected: usize,
    sort_by: SortBy,
    view_size_idx: usize,
    grouped: bool,
    status: Option<(String, Instant)>,
    pending_kill: Option<PendingKill>,
}

pub enum Outcome {
    Continue,
    Quit,
}

impl App {
    pub fn new() -> Self {
        let num_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .max(1) as f64;
        Self {
            procs: HashMap::new(),
            window_start: Instant::now(),
            first_snapshot: false,
            num_cores,
            selected: 0,
            sort_by: SortBy::Cpu,
            view_size_idx: 0,
            grouped: true,
            status: None,
            pending_kill: None,
        }
    }

    /// Build an app by blocking-sampling the live system `secs` times (min 2,
    /// since CPU% needs a delta between samples) spaced by `interval`. Used by
    /// one-shot snapshot mode; nothing runs concurrently, so it just blocks.
    pub fn sampled(secs: u64, interval: Duration) -> Self {
        let mut sampler = Sampler::new();
        let mut app = App::new();
        app.ingest(sampler.sample()); // baseline
        for _ in 1..secs.max(2) {
            std::thread::sleep(interval);
            app.ingest(sampler.sample());
        }
        app
    }

    pub fn top_n(&self) -> usize {
        VIEW_SIZES[self.view_size_idx]
    }

    fn cycle_view_size(&mut self) {
        self.view_size_idx = (self.view_size_idx + 1) % VIEW_SIZES.len();
        self.selected = 0;
    }

    pub fn ingest(&mut self, snapshot: Snapshot) {
        let is_initial = !self.first_snapshot;
        if is_initial {
            self.window_start = snapshot.taken_at;
            self.first_snapshot = true;
        }
        let live: HashSet<Pid> = snapshot.procs.iter().map(|p| p.pid).collect();
        self.procs.retain(|pid, _| live.contains(pid));
        for p in snapshot.procs {
            self.procs
                .entry(p.pid)
                .and_modify(|s| {
                    s.last_cpu_ms = p.cpu_ms;
                    s.last_seen = snapshot.taken_at;
                    s.last_memory_bytes = p.memory_bytes;
                    s.memory_sum_bytes += p.memory_bytes as u128;
                    s.memory_samples += 1;
                })
                .or_insert_with(|| ProcState {
                    cmd: p.cmd,
                    group: p.group,
                    platform: p.platform,
                    baseline_cpu_ms: p.cpu_ms,
                    baseline_at: snapshot.taken_at,
                    last_cpu_ms: p.cpu_ms,
                    last_seen: snapshot.taken_at,
                    last_memory_bytes: p.memory_bytes,
                    memory_sum_bytes: p.memory_bytes as u128,
                    memory_samples: 1,
                    is_new: !is_initial,
                });
        }
    }

    pub fn apply(&mut self, action: Action, ctrl: &impl ProcessController) -> Outcome {
        match action {
            Action::Quit => return Outcome::Quit,
            Action::Reset => {
                self.reset_window();
                self.set_status("Window reset");
            }
            Action::SelectNext => {
                let max = self.rank_top(self.top_n()).len().saturating_sub(1);
                self.selected = (self.selected + 1).min(max);
            }
            Action::SelectPrev => {
                self.selected = self.selected.saturating_sub(1);
            }
            Action::RequestKill(signal) => {
                let rows = self.rank_top(self.top_n());
                let idx = self.selected.min(rows.len().saturating_sub(1));
                if let Some(row) = rows.get(idx) {
                    let pending = PendingKill {
                        pids: row.pids.clone(),
                        signal,
                        label: row.label.clone(),
                    };
                    drop(rows);
                    self.selected = idx;
                    self.pending_kill = Some(pending);
                }
            }
            Action::ConfirmKill => {
                if let Some(pk) = self.pending_kill.take() {
                    let report = ctrl.signal_all(&pk.pids, pk.signal);
                    self.set_status(&report.summary(&pk.label, pk.signal, pk.pids.len()));
                }
            }
            Action::CancelKill => {
                self.pending_kill = None;
                self.set_status("Cancelled");
            }
            Action::SortBy(mode) => {
                if self.sort_by != mode {
                    self.sort_by = mode;
                    self.selected = 0;
                    self.set_status(&format!("Sorted by {}", mode.label()));
                }
            }
            Action::CycleViewSize => {
                self.cycle_view_size();
                self.set_status(&format!("Showing top {}", self.top_n()));
            }
            Action::ToggleGroup => {
                self.grouped = !self.grouped;
                self.selected = 0;
                self.set_status(if self.grouped {
                    "Grouping Electron apps"
                } else {
                    "Ungrouped (per-process)"
                });
            }
        }
        Outcome::Continue
    }

    pub fn rank_top(&self, n: usize) -> Vec<Row> {
        let mut singles: Vec<Row> = Vec::new();
        let mut groups: HashMap<&str, Row> = HashMap::new();

        for (pid, p) in &self.procs {
            let Some(cpu_pct) = p.cpu_pct(self.num_cores) else {
                continue;
            };
            let mem = p.avg_memory_bytes();

            match (self.grouped, &p.group) {
                (true, Some(group)) => {
                    let row = groups.entry(group.as_str()).or_insert_with(|| Row {
                        label: group.clone(),
                        cpu_pct: 0.0,
                        avg_memory_bytes: 0,
                        pids: Vec::new(),
                        is_group: true,
                        is_new: false,
                        platform: p.platform,
                    });
                    row.cpu_pct += cpu_pct;
                    row.avg_memory_bytes += mem;
                    row.pids.push(*pid);
                    row.is_new |= p.is_new;
                }
                _ => singles.push(Row {
                    label: p.cmd.clone(),
                    cpu_pct,
                    avg_memory_bytes: mem,
                    pids: vec![*pid],
                    is_group: false,
                    is_new: p.is_new,
                    platform: p.platform,
                }),
            }
        }

        let mut rows: Vec<Row> = singles.into_iter().chain(groups.into_values()).collect();
        match self.sort_by {
            SortBy::Cpu => rows.sort_by(|a, b| {
                b.cpu_pct
                    .partial_cmp(&a.cpu_pct)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            SortBy::Memory => rows.sort_by(|a, b| b.avg_memory_bytes.cmp(&a.avg_memory_bytes)),
        }
        rows.truncate(n);
        rows
    }

    pub fn expire_status(&mut self) {
        if let Some((_, at)) = &self.status
            && at.elapsed() > STATUS_TTL
        {
            self.status = None;
        }
    }

    pub fn window_elapsed(&self) -> Duration {
        if self.first_snapshot {
            Instant::now().duration_since(self.window_start)
        } else {
            Duration::ZERO
        }
    }

    pub fn proc_count(&self) -> usize {
        self.procs.len()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn sort_by(&self) -> SortBy {
        self.sort_by
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_ref().map(|(s, _)| s.as_str())
    }

    pub fn pending_kill(&self) -> Option<&PendingKill> {
        self.pending_kill.as_ref()
    }

    pub fn has_pending_kill(&self) -> bool {
        self.pending_kill.is_some()
    }

    pub fn grouped(&self) -> bool {
        self.grouped
    }

    fn reset_window(&mut self) {
        let now = Instant::now();
        for p in self.procs.values_mut() {
            p.baseline_cpu_ms = p.last_cpu_ms;
            p.baseline_at = now;
            p.memory_sum_bytes = p.last_memory_bytes as u128;
            p.memory_samples = 1;
            p.is_new = false;
        }
        self.window_start = now;
    }

    fn set_status(&mut self, msg: &str) {
        self.status = Some((msg.to_string(), Instant::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampler::{ProcSnapshot, Snapshot};
    use std::cell::RefCell;

    /// Fake controller that records signals instead of sending them.
    #[derive(Default)]
    struct RecordingController {
        sent: RefCell<Vec<(Pid, Signal)>>,
    }
    impl ProcessController for RecordingController {
        fn signal(&self, pid: Pid, signal: Signal) -> anyhow::Result<()> {
            self.sent.borrow_mut().push((pid, signal));
            Ok(())
        }
    }

    struct FailingController;
    impl ProcessController for FailingController {
        fn signal(&self, _pid: Pid, _signal: Signal) -> anyhow::Result<()> {
            anyhow::bail!("no such process")
        }
    }

    /// Build an App holding one rankable process. Two snapshots 0.6s apart
    /// (via `Instant + Duration`, no real sleep) clear the MIN_WINDOW_SECS gate.
    fn app_with_one_proc(pid: u32) -> App {
        let t0 = Instant::now();
        let mk = |t: Instant| Snapshot {
            taken_at: t,
            procs: vec![ProcSnapshot {
                pid: Pid::from_u32(pid),
                cmd: "x".into(),
                cpu_ms: 0,
                memory_bytes: 0,
                group: None,
                platform: Platform::Other,
            }],
        };
        let mut app = App::new();
        app.ingest(mk(t0));
        app.ingest(mk(t0 + Duration::from_millis(600)));
        app
    }

    #[test]
    fn confirm_kill_signals_selected_pid_once() {
        let mut app = app_with_one_proc(4242);
        let ctrl = RecordingController::default();

        app.apply(Action::RequestKill(Signal::SIGTERM), &ctrl);
        assert_eq!(
            app.pending_kill()
                .map(|p| (p.pids.clone(), p.signal)),
            Some((vec![Pid::from_u32(4242)], Signal::SIGTERM))
        );

        app.apply(Action::ConfirmKill, &ctrl);
        assert_eq!(
            *ctrl.sent.borrow(),
            vec![(Pid::from_u32(4242), Signal::SIGTERM)]
        );
        assert!(app.pending_kill().is_none());
    }

    #[test]
    fn force_kill_uses_sigkill() {
        let mut app = app_with_one_proc(7);
        let ctrl = RecordingController::default();
        app.apply(Action::RequestKill(Signal::SIGKILL), &ctrl);
        app.apply(Action::ConfirmKill, &ctrl);
        assert_eq!(ctrl.sent.borrow()[0].1, Signal::SIGKILL);
    }

    #[test]
    fn pure_actions_never_signal() {
        let mut app = app_with_one_proc(1);
        let ctrl = RecordingController::default();
        app.apply(Action::SelectNext, &ctrl);
        app.apply(Action::CycleViewSize, &ctrl);
        app.apply(Action::SortBy(SortBy::Memory), &ctrl);
        assert!(ctrl.sent.borrow().is_empty());
    }

    #[test]
    fn signal_error_sets_status_without_panic() {
        let mut app = app_with_one_proc(99);
        app.apply(Action::RequestKill(Signal::SIGTERM), &FailingController);
        app.apply(Action::ConfirmKill, &FailingController);
        assert!(app.status().unwrap().contains("Failed"));
    }

    /// Build an app with the given (pid, group) processes, sampled twice.
    fn app_with_procs(procs: &[(u32, Option<&str>)]) -> App {
        let t0 = Instant::now();
        let mk = |t: Instant| Snapshot {
            taken_at: t,
            procs: procs
                .iter()
                .map(|(pid, group)| ProcSnapshot {
                    pid: Pid::from_u32(*pid),
                    cmd: group.unwrap_or("proc").to_string(),
                    cpu_ms: 0,
                    memory_bytes: 0,
                    group: group.map(str::to_string),
                    platform: if group.is_some() {
                        Platform::Electron
                    } else {
                        Platform::Other
                    },
                })
                .collect(),
        };
        let mut app = App::new();
        app.ingest(mk(t0));
        app.ingest(mk(t0 + Duration::from_millis(600)));
        app
    }

    #[test]
    fn electron_group_sums_and_kills_all_members() {
        let mut app = app_with_procs(&[(100, Some("Bitwarden")), (101, Some("Bitwarden"))]);

        let rows = app.rank_top(10);
        assert_eq!(rows.len(), 1, "two procs collapse to one group row");
        assert!(rows[0].label.contains("Bitwarden"));
        assert!(rows[0].is_group);
        assert_eq!(rows[0].pids.len(), 2);

        let ctrl = RecordingController::default();
        app.apply(Action::RequestKill(Signal::SIGTERM), &ctrl);
        app.apply(Action::ConfirmKill, &ctrl);
        let sent: Vec<Pid> = ctrl.sent.borrow().iter().map(|(p, _)| *p).collect();
        assert_eq!(sent.len(), 2, "killing the group signals every member");
        assert!(sent.contains(&Pid::from_u32(100)));
        assert!(sent.contains(&Pid::from_u32(101)));
    }

    #[test]
    fn toggling_group_off_shows_each_process() {
        let mut app = app_with_procs(&[(100, Some("Bitwarden")), (101, Some("Bitwarden"))]);
        assert_eq!(app.rank_top(10).len(), 1, "grouped by default");

        app.apply(Action::ToggleGroup, &RecordingController::default());
        let rows = app.rank_top(10);
        assert_eq!(rows.len(), 2, "ungrouped → one row per process");
        assert!(rows.iter().all(|r| !r.is_group));
    }
}
