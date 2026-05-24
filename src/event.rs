use anyhow::Result;
use nix::sys::fanotify::{FanotifyEvent, FanotifyResponse, MaskFlags, Response};
use std::os::fd::{AsRawFd, BorrowedFd};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

use crate::config::Watch;
use crate::watcher::Watcher;

pub enum Mode {
    Enforce,
    WatchOnly,
}

pub struct AllowIndex {
    entries: Vec<(PathBuf, Vec<PathBuf>)>,
}

impl AllowIndex {
    pub fn from_watches(watches: &[Watch]) -> Result<Self> {
        let mut entries = Vec::with_capacity(watches.len());
        for w in watches {
            let paths = w.allow_processes.iter().map(PathBuf::from).collect();
            entries.push((w.resolved_path()?, paths));
        }
        Ok(Self { entries })
    }

    fn allows(&self, file_path: &Path, exe_path: Option<&Path>) -> bool {
        let mut best: Option<&Vec<PathBuf>> = None;
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
            Some(allow) => match exe_path {
                Some(exe) => allow.iter().any(|p| p == exe),
                None => false,
            },
            None => true,
        }
    }
}

pub fn run_loop(watcher: &Watcher, index: &AllowIndex, mode: Mode) -> Result<()> {
    loop {
        let events = watcher.group.read_events()?;
        for ev in events {
            handle_event(watcher, index, &mode, &ev);
        }
    }
}

fn handle_event(watcher: &Watcher, index: &AllowIndex, mode: &Mode, ev: &FanotifyEvent) {
    let Some(fd) = ev.fd() else {
        warn!("fanotify queue overflow");
        return;
    };

    let file_path = resolve_fd_path(fd).unwrap_or_else(|| PathBuf::from("<unknown>"));
    let pid = ev.pid();
    let exe_path = read_exe(pid);
    let mask = ev.mask();

    let allowed = index.allows(&file_path, exe_path.as_deref());

    if allowed {
        info!(
            exe = ?exe_path,
            pid,
            path = %file_path.display(),
            ?mask,
            "access allowed"
        );
    } else if matches!(mode, Mode::Enforce) {
        warn!(
            exe = ?exe_path,
            pid,
            path = %file_path.display(),
            ?mask,
            "BLOCKED unauthorized access"
        );
    } else {
        warn!(
            exe = ?exe_path,
            pid,
            path = %file_path.display(),
            ?mask,
            "UNAUTHORIZED access (watch-only: allowed)"
        );
    }

    if mask.intersects(
        MaskFlags::FAN_OPEN_PERM | MaskFlags::FAN_ACCESS_PERM | MaskFlags::FAN_OPEN_EXEC_PERM,
    ) {
        let response = if !allowed && matches!(mode, Mode::Enforce) {
            FanotifyResponse::new(fd, Response::FAN_DENY)
        } else {
            FanotifyResponse::new(fd, Response::FAN_ALLOW)
        };
        if let Err(e) = watcher.group.write_response(response) {
            warn!(error = %e, "failed to write fanotify response");
        }
    }
}

fn resolve_fd_path(fd: BorrowedFd<'_>) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd())).ok()
}

fn read_exe(pid: i32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}
