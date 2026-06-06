use std::collections::HashMap;
use std::time::{Duration, Instant};

use sysinfo::{Pid, Process, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tokio::sync::mpsc;

use crate::classifier::{self, Platform};

const CHANNEL_CAPACITY: usize = 8;

pub struct Snapshot {
    pub taken_at: Instant,
    pub procs: Vec<ProcSnapshot>,
}

pub struct ProcSnapshot {
    pub pid: Pid,
    pub cmd: String,
    pub cpu_ms: u64,
    pub memory_bytes: u64,
    /// Electron app this process belongs to (own identity or inherited from an
    /// ancestor), for app-level grouping. `None` for Chrome and non-app procs.
    pub group: Option<String>,
    /// The process's runtime family.
    pub platform: Platform,
}

/// Synchronous sampling primitive: holds the stateful sysinfo `System` and
/// produces one `Snapshot` per `sample()`. Use it directly for one-shot/batch
/// collection, or via `spawn` for the streaming interactive case.
pub struct Sampler {
    system: System,
    refresh: ProcessRefreshKind,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            // CPU + memory refresh every tick; cmd only needs fetching once
            // (a process's command line is fixed for its lifetime).
            refresh: ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_cmd(UpdateKind::OnlyIfNotSet),
        }
    }

    pub fn sample(&mut self) -> Snapshot {
        self.system
            .refresh_processes_specifics(ProcessesToUpdate::All, true, self.refresh);
        self.build_snapshot()
    }

    /// Stream snapshots on a background blocking thread, returning the channel
    /// it feeds. Consumes the sampler — it moves onto the worker thread for the
    /// app's lifetime, decoupling slow sampling from the interactive event loop.
    /// For one-shot collection, call [`sample`](Self::sample) directly instead.
    pub fn spawn(mut self, interval: Duration) -> mpsc::Receiver<Snapshot> {
        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        tokio::task::spawn_blocking(move || loop {
            if tx.blocking_send(self.sample()).is_err() {
                return;
            }
            std::thread::sleep(interval);
        });
        rx
    }
}

/// Per-process facts gathered in the first pass, used to resolve labels and
/// group keys with parent inheritance in the second.
struct Proc {
    /// argv as owned strings — we keep the original tokenization from sysinfo
    /// because joining+resplitting on whitespace destroys macOS paths whose
    /// argv[0] contains spaces (e.g. `/Applications/Google Chrome.app/...`).
    argv: Vec<String>,
    parent: Option<Pid>,
    /// Electron app identity from this process's own argv, if any.
    app: Option<String>,
    cpu_ms: u64,
    memory_bytes: u64,
}

impl Sampler {
    /// Turn the current (already-refreshed) process table into a snapshot.
    fn build_snapshot(&self) -> Snapshot {
        let system = &self.system;
        // Pass 1: gather facts per *process*. Skip threads — sysinfo lists a
        // process's threads here too, and each thread reports the whole process's
        // memory, which would inflate counts and double-count RSS when grouping.
        let procs_by_pid: HashMap<Pid, Proc> = system
            .processes()
            .iter()
            .filter(|(_, p)| p.thread_kind().is_none())
            .map(|(pid, p)| {
                let argv = argv_strings(p);
                let argv_refs: Vec<&str> = argv.iter().map(String::as_str).collect();
                let app = classifier::group_app(&argv_refs);
                (
                    *pid,
                    Proc {
                        argv,
                        parent: p.parent(),
                        app,
                        cpu_ms: p.accumulated_cpu_time(),
                        memory_bytes: p.memory(),
                    },
                )
            })
            .collect();

        // Pass 2: resolve each process's label and group key. Self-identifying
        // Electron procs use their own app; generic Electron children inherit it
        // from the nearest ancestor that has one (the main process holds the .asar
        // identity). Only Electron-family procs group — so non-Electron children of
        // an Electron app (e.g. a shell from VS Code's terminal) stay standalone.
        let procs = procs_by_pid
            .iter()
            .map(|(pid, proc)| {
                let argv: Vec<&str> = proc.argv.iter().map(String::as_str).collect();
                let groupable = classifier::is_groupable_family(&argv);
                let inherited = if groupable && proc.app.is_none() {
                    inherited_app(*pid, &procs_by_pid)
                } else {
                    None
                };
                let group = if groupable {
                    proc.app.clone().or_else(|| inherited.clone())
                } else {
                    None
                };
                let cmd = if proc.app.is_some() {
                    classifier::friendly_name(&argv)
                } else if let Some(app) = &inherited {
                    classifier::inherited_label(app, &argv)
                } else {
                    classifier::friendly_name(&argv)
                };
                ProcSnapshot {
                    pid: *pid,
                    cmd,
                    cpu_ms: proc.cpu_ms,
                    memory_bytes: proc.memory_bytes,
                    group,
                    platform: classifier::platform(&argv),
                }
            })
            .collect();
        Snapshot {
            taken_at: Instant::now(),
            procs,
        }
    }
}

fn argv_strings(p: &Process) -> Vec<String> {
    if p.cmd().is_empty() {
        // Kernel threads / processes with no readable cmdline. Box-bracketed,
        // single token (the comm name) — preserves the empty-argv signal while
        // giving classifiers something to display.
        vec![format!("[{}]", p.name().to_string_lossy())]
    } else {
        p.cmd()
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }
}

/// Walk the parent chain from `pid` and return the first ancestor's
/// self-identified app name. Used so generic Electron children inherit the app
/// name held by their main process.
fn inherited_app(pid: Pid, procs: &HashMap<Pid, Proc>) -> Option<String> {
    let mut cursor = procs.get(&pid)?.parent;
    let mut hops = 0;
    while let Some(ppid) = cursor {
        let parent = procs.get(&ppid)?;
        if let Some(app) = &parent.app {
            return Some(app.clone());
        }
        cursor = parent.parent;
        hops += 1;
        if hops > 32 {
            break; // guard against cycles / pathological trees
        }
    }
    None
}