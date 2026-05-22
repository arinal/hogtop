use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid as NixPid;
use sysinfo::Pid;

use crate::event::Action;
use crate::sampler::Snapshot;

pub const VIEW_SIZES: [usize; 3] = [10, 50, 100];
const STATUS_TTL: Duration = Duration::from_secs(3);
const MIN_WINDOW_SECS: f64 = 0.5;

pub struct ProcState {
    pub cmd: String,
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
}

pub struct RankedProc<'a> {
    pub pid: Pid,
    pub state: &'a ProcState,
    pub cpu_pct: f64,
    pub avg_memory_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct PendingKill {
    pub pid: Pid,
    pub signal: Signal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Cpu,
    Memory,
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
            status: None,
            pending_kill: None,
        }
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

    pub fn apply(&mut self, action: Action) -> Outcome {
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
                let ranked = self.rank_top(self.top_n());
                let idx = self.selected.min(ranked.len().saturating_sub(1));
                let pid_opt = ranked.get(idx).map(|r| r.pid);
                drop(ranked);
                if let Some(pid) = pid_opt {
                    self.selected = idx;
                    self.pending_kill = Some(PendingKill { pid, signal });
                }
            }
            Action::ConfirmKill => {
                if let Some(pk) = self.pending_kill.take() {
                    let msg = match kill(NixPid::from_raw(pk.pid.as_u32() as i32), pk.signal) {
                        Ok(()) => format!("Sent {} to PID {}", pk.signal.as_str(), pk.pid),
                        Err(e) => format!("Failed to signal {}: {}", pk.pid, e),
                    };
                    self.set_status(&msg);
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
        }
        Outcome::Continue
    }

    pub fn rank_top(&self, n: usize) -> Vec<RankedProc<'_>> {
        let mut metrics: Vec<(Pid, f64, u64)> = self
            .procs
            .iter()
            .filter_map(|(pid, p)| {
                let elapsed = p
                    .last_seen
                    .saturating_duration_since(p.baseline_at)
                    .as_secs_f64();
                if elapsed < MIN_WINDOW_SECS {
                    return None;
                }
                let cpu_secs = p.last_cpu_ms.saturating_sub(p.baseline_cpu_ms) as f64 / 1000.0;
                let cpu_pct = cpu_secs / elapsed / self.num_cores * 100.0;
                Some((*pid, cpu_pct, p.avg_memory_bytes()))
            })
            .collect();
        match self.sort_by {
            SortBy::Cpu => metrics.sort_by(|a, b| {
                b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
            }),
            SortBy::Memory => metrics.sort_by(|a, b| b.2.cmp(&a.2)),
        }
        metrics.truncate(n);

        metrics
            .into_iter()
            .filter_map(|(pid, cpu_pct, avg_memory_bytes)| {
                self.procs.get(&pid).map(|state| RankedProc {
                    pid,
                    state,
                    cpu_pct,
                    avg_memory_bytes,
                })
            })
            .collect()
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

    pub fn pending_kill(&self) -> Option<PendingKill> {
        self.pending_kill
    }

    pub fn has_pending_kill(&self) -> bool {
        self.pending_kill.is_some()
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
