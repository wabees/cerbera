use anyhow::Result;
use nix::sys::fanotify::{FanotifyEvent, FanotifyResponse, MaskFlags, Response};
use std::os::fd::{AsRawFd, BorrowedFd};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

use crate::config::Watch;
use crate::watcher::Watcher;

pub struct AllowIndex {
    entries: Vec<(PathBuf, Vec<String>)>,
}

impl AllowIndex {
    pub fn from_watches(watches: &[Watch]) -> Result<Self> {
        let mut entries = Vec::with_capacity(watches.len());
        for w in watches {
            entries.push((w.resolved_path()?, w.allow_processes.clone()));
        }
        Ok(Self { entries })
    }

    fn allows(&self, file_path: &Path, process_name: &str) -> bool {
        let mut best: Option<&Vec<String>> = None;
        let mut best_len = 0usize;
        for (root, allow) in &self.entries {
            if file_path.starts_with(root) {
                let len = root.as_os_str().len();
                if len >= best_len {
                    best_len = len;
                    best = Some(allow);
                }
            }
        }
        match best {
            Some(allow) => allow.iter().any(|p| process_name.contains(p.as_str())),
            None => true,
        }
    }
}

pub fn run_loop(watcher: &Watcher, index: &AllowIndex) -> Result<()> {
    loop {
        let events = watcher.group.read_events()?;
        for ev in events {
            handle_event(watcher, index, &ev);
        }
    }
}

fn handle_event(watcher: &Watcher, index: &AllowIndex, ev: &FanotifyEvent) {
    let Some(fd) = ev.fd() else {
        warn!("fanotify queue overflow");
        return;
    };

    let file_path = resolve_fd_path(fd).unwrap_or_else(|| PathBuf::from("<unknown>"));
    let pid = ev.pid();
    let proc_name = read_comm(pid).unwrap_or_else(|| "<unknown>".into());
    let mask = ev.mask();

    let allowed = index.allows(&file_path, &proc_name);

    if allowed {
        info!(
            process = %proc_name,
            pid,
            path = %file_path.display(),
            ?mask,
            "access"
        );
    } else {
        warn!(
            process = %proc_name,
            pid,
            path = %file_path.display(),
            ?mask,
            "UNAUTHORIZED access (watch-only: allowed)"
        );
    }

    if mask.intersects(
        MaskFlags::FAN_OPEN_PERM | MaskFlags::FAN_ACCESS_PERM | MaskFlags::FAN_OPEN_EXEC_PERM,
    ) {
        let response = FanotifyResponse::new(fd, Response::FAN_ALLOW);
        if let Err(e) = watcher.group.write_response(response) {
            warn!(error = %e, "failed to write FAN_ALLOW response");
        }
    }
}

fn resolve_fd_path(fd: BorrowedFd<'_>) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd())).ok()
}

fn read_comm(pid: i32) -> Option<String> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    Some(s.trim().to_string())
}
