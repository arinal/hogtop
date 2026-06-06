use anyhow::Result;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid as NixPid;
use sysinfo::Pid;

/// Outcome of signalling a group of processes: how many succeeded and the last
/// error encountered (if any).
pub struct KillReport {
    pub sent: usize,
    pub last_err: Option<anyhow::Error>,
}

impl KillReport {
    /// A human status line for the kill, e.g. "Sent SIGTERM to Bitwarden
    /// (8 procs)" or "Failed to signal Chrome: ...". `requested` is how many
    /// processes the kill targeted (to phrase single vs. group).
    pub fn summary(&self, label: &str, signal: Signal, requested: usize) -> String {
        match &self.last_err {
            None if requested == 1 => format!("Sent {} to {}", signal.as_str(), label),
            None => format!("Sent {} to {} ({} procs)", signal.as_str(), label, self.sent),
            Some(e) => format!("Failed to signal {}: {}", label, e),
        }
    }
}

/// Port (algebra) for sending signals to processes.
///
/// The value here is the trait itself: it is the seam that lets tests
/// substitute a fake and assert the selection → pid → signal logic without
/// signalling real processes. The production implementor is a trivial adapter.
pub trait ProcessController {
    fn signal(&self, pid: Pid, signal: Signal) -> Result<()>;

    /// Signal every pid in `pids`, tallying successes and the last error.
    /// Lives here (not in the caller) so the core just expresses intent —
    /// "kill these" — and the controller owns the iteration. Default impl
    /// loops over [`signal`](Self::signal); implementors rarely override.
    fn signal_all(&self, pids: &[Pid], signal: Signal) -> KillReport {
        pids.iter().fold(
            KillReport {
                sent: 0,
                last_err: None,
            },
            |mut report, &pid| {
                match self.signal(pid, signal) {
                    Ok(()) => report.sent += 1,
                    Err(e) => report.last_err = Some(e),
                }
                report
            },
        )
    }
}

/// Production interpreter: a one-line forward to `nix::sys::signal::kill`.
pub struct Nix;

impl ProcessController for Nix {
    fn signal(&self, pid: Pid, signal: Signal) -> Result<()> {
        kill(NixPid::from_raw(pid.as_u32() as i32), signal)?;
        Ok(())
    }
}