use std::collections::HashMap;
use std::time::{Duration, Instant};

use sysinfo::{Pid, Process, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tokio::sync::mpsc;

use crate::classifier::{self, AppId, Platform};

const CHANNEL_CAPACITY: usize = 8;

pub struct Snapshot {
    pub taken_at: Instant,
    pub procs: Vec<ProcSnapshot>,
    /// Total physical RAM, for sizing memory as a fraction of the system.
    pub total_memory: u64,
    /// RAM currently in use, system-wide.
    pub used_memory: u64,
    /// System-wide CPU usage, 0.0..=100.0 (averaged across cores).
    pub cpu_usage: f32,
}

pub struct ProcSnapshot {
    pub pid: Pid,
    pub cmd: String,
    /// Absolute path to the executable (empty if the kernel won't reveal it).
    pub exe: String,
    pub cpu_ms: u64,
    pub memory_bytes: u64,
    /// Electron app this process belongs to (own identity or inherited from an
    /// ancestor), for app-level grouping. `None` for Chrome and non-app procs.
    pub group: Option<String>,
    /// The process's runtime family.
    pub platform: Platform,
    /// The recognised app this process belongs to, if any — a domain fact the
    /// UI keys icons off. Derived from the resolved `group` identity, so it's
    /// set for every process of a known app (grouped or per-process), and
    /// `None` for Chrome/Firefox/unrecognised procs.
    pub app: Option<AppId>,
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
            // CPU + memory refresh every tick; cmd and exe only need fetching
            // once (a process's command line and binary are fixed for its life).
            refresh: ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_exe(UpdateKind::OnlyIfNotSet),
        }
    }

    pub fn sample(&mut self) -> Snapshot {
        // System-wide CPU% and RAM totals, alongside the per-process table.
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
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
    /// Absolute executable path (empty if the kernel won't reveal it).
    exe: String,
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
                        exe: p
                            .exe()
                            .map(|e| e.to_string_lossy().into_owned())
                            .unwrap_or_default(),
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
                // The recognised-app fact, keyed off the resolved identity, so a
                // known app's icon shows on every one of its rows (grouped or
                // per-process). `None` for Chrome/Firefox (their group name is a
                // platform, not a registry app) and unrecognised procs.
                let app = group.as_deref().and_then(classifier::app_id);
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
                    exe: proc.exe.clone(),
                    cpu_ms: proc.cpu_ms,
                    memory_bytes: proc.memory_bytes,
                    group,
                    platform: classifier::platform(&argv),
                    app,
                }
            })
            .collect();
        Snapshot {
            taken_at: Instant::now(),
            procs,
            total_memory: system.total_memory(),
            used_memory: system.used_memory(),
            cpu_usage: system.global_cpu_usage(),
        }
    }
}

fn argv_strings(p: &Process) -> Vec<String> {
    if p.cmd().is_empty() {
        // Kernel threads / processes with no readable cmdline. Box-bracketed,
        // single token (the comm name) — preserves the empty-argv signal while
        // giving classifiers something to display.
        return vec![format!("[{}]", p.name().to_string_lossy())];
    }
    let cmd: Vec<String> = p.cmd().iter().map(|s| s.to_string_lossy().into_owned()).collect();
    let exe = p.exe().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    detokenize(cmd, &exe)
}

/// Chromium-based apps (Chrome, every Electron app, Spotify, …) rewrite their
/// own `/proc/self/cmdline` into a single space-joined string to set a friendly
/// process title, erasing the NUL separators the kernel normally uses. sysinfo
/// then returns the whole command line as one `argv[0]` element, which wrecks
/// classification: `exe_basename` runs `Path::file_name` on the entire string
/// and yields garbage (a Spotify renderer's "basename" comes out as the tail
/// after `…Spotify/1.2.90.451`). When the lone element is the executable path
/// followed by more text, re-split the trailing args on whitespace so the
/// classifier sees real tokens again. argv[0] is peeled off as the known `exe`
/// path first, so a macOS path that legitimately contains spaces (e.g.
/// `…/Code - Insiders.app/…`) stays whole and is never re-split.
fn detokenize(cmd: Vec<String>, exe: &str) -> Vec<String> {
    if cmd.len() != 1 || exe.is_empty() {
        return cmd;
    }
    // Require a clean boundary — the exe path then whitespace — so we don't
    // mistake a longer path that merely shares this prefix for a rewrite. An
    // exact match (no trailing text, e.g. an args-less main process) leaves the
    // single element untouched.
    let rest = match cmd[0].strip_prefix(exe) {
        Some(r) if r.starts_with(char::is_whitespace) => r,
        _ => return cmd,
    };
    std::iter::once(exe.to_string())
        .chain(rest.split_whitespace().map(str::to_string))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::detokenize;

    fn v(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn splits_chromium_rewritten_cmdline() {
        // The real shape of the Spotify-renderer bug: the whole command line
        // arrives as a single space-joined element. We split it back into the
        // exe path plus its flags so the classifier can recognise it.
        let exe = "/home/me/.local/share/spotify-launcher/install/usr/share/spotify/spotify";
        let joined = format!(
            "{exe} --type=renderer --user-agent-product=Chrome/146 Spotify/1.2.90 \
             --no-sandbox --autoplay-policy=no-user-gesture-required"
        );
        let got = detokenize(v(&[&joined]), exe);
        assert_eq!(got[0], exe);
        assert_eq!(got[1], "--type=renderer");
        // exe basename survives for classification…
        assert_eq!(
            std::path::Path::new(&got[0]).file_name().unwrap().to_str().unwrap(),
            "spotify"
        );
    }

    #[test]
    fn leaves_args_less_main_process_untouched() {
        // A main process with no args is a single element equal to the exe —
        // nothing to split, and we must not turn it into anything else.
        let exe = "/usr/share/spotify/spotify";
        assert_eq!(detokenize(v(&[exe]), exe), v(&[exe]));
    }

    #[test]
    fn leaves_already_tokenized_cmdline_untouched() {
        // The normal kernel case: NUL-separated argv arrives pre-split.
        let exe = "/usr/share/spotify/spotify";
        let argv = v(&[exe, "--type=zygote", "--no-sandbox"]);
        assert_eq!(detokenize(argv.clone(), exe), argv);
    }

    #[test]
    fn preserves_macos_exe_path_with_spaces() {
        // macOS argv[0] legitimately contains spaces. It comes as a single
        // element that exactly equals the exe, so it stays whole — never split
        // on the spaces inside "Code - Insiders".
        let exe = "/Applications/Visual Studio Code - Insiders.app/Contents/MacOS/Electron";
        assert_eq!(detokenize(v(&[exe]), exe), v(&[exe]));
    }

    #[test]
    fn leaves_single_element_alone_without_exe() {
        // No exe path to anchor on (kernel wouldn't reveal it): don't guess.
        let argv = v(&["1.2.90.451 --no-sandbox"]);
        assert_eq!(detokenize(argv.clone(), ""), argv);
    }
}