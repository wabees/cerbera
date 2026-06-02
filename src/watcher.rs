use anyhow::{Context, Result};
use nix::sys::fanotify::{EventFFlags, Fanotify, InitFlags, MarkFlags, MaskFlags};
use std::path::Path;
use tracing::{debug, warn};

use crate::config::Watch;

pub struct Watcher {
    pub group: Fanotify,
}

impl Watcher {
    pub fn new() -> Result<Self> {
        let group = Fanotify::init(
            InitFlags::FAN_CLOEXEC | InitFlags::FAN_CLASS_CONTENT,
            EventFFlags::O_RDONLY | EventFFlags::O_LARGEFILE,
        )
        .context("fanotify_init failed (CAP_SYS_ADMIN / root required)")?;
        Ok(Self { group })
    }

    pub fn add_watch(&self, watch: &Watch) -> Result<()> {
        let path = watch.resolved_path()?;
        if !path.exists() {
            warn!(watch = %watch.name, path = %path.display(), "path does not exist, skipping");
            return Ok(());
        }
        let mask = MaskFlags::FAN_OPEN_PERM | MaskFlags::FAN_ACCESS_PERM;

        self.mark_path(&path, mask)
            .with_context(|| format!("failed to mark {}", path.display()))?;
        debug!(watch = %watch.name, path = %path.display(), "marked");

        if path.is_dir() {
            self.mark_tree(&path, mask)?;
        }
        Ok(())
    }

    fn mark_path(&self, path: &Path, mask: MaskFlags) -> Result<()> {
        self.group
            .mark(MarkFlags::FAN_MARK_ADD, mask, None, Some(path))?;
        Ok(())
    }

    fn mark_tree(&self, dir: &Path, mask: MaskFlags) -> Result<()> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, dir = %dir.display(), "cannot read dir");
                return Ok(());
            }
        };
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let entry_path = entry.path();
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if let Err(e) = self.mark_path(&entry_path, mask) {
                debug!(error = ?e, path = %entry_path.display(), "skip mark");
            }
            if ft.is_dir() {
                let _ = self.mark_tree(&entry_path, mask);
            }
        }
        Ok(())
    }
}
