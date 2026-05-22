use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessesToUpdate, System};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::identify::{friendly_name, icon_for};

pub struct Snapshot {
    pub taken_at: Instant,
    pub procs: Vec<ProcSnapshot>,
}

pub struct ProcSnapshot {
    pub pid: Pid,
    pub cmd: String,
    pub cpu_ms: u64,
    pub memory_bytes: u64,
}

pub fn spawn(tx: mpsc::Sender<Snapshot>, interval: Duration) -> JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        let mut system = System::new();
        loop {
            system.refresh_processes(ProcessesToUpdate::All, true);
            let snapshot = build_snapshot(&system);
            if tx.blocking_send(snapshot).is_err() {
                return;
            }
            std::thread::sleep(interval);
        }
    })
}

fn build_snapshot(system: &System) -> Snapshot {
    let procs = system
        .processes()
        .iter()
        .map(|(pid, p)| {
            let raw_cmd = if p.cmd().is_empty() {
                format!("[{}]", p.name().to_string_lossy())
            } else {
                p.cmd()
                    .iter()
                    .map(|s| s.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let friendly = friendly_name(&raw_cmd);
            let icon = icon_for(&friendly);
            let cmd = format!("{} {}", icon, friendly);
            ProcSnapshot {
                pid: *pid,
                cmd,
                cpu_ms: p.accumulated_cpu_time(),
                memory_bytes: p.memory(),
            }
        })
        .collect();
    Snapshot {
        taken_at: Instant::now(),
        procs,
    }
}
