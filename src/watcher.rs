use crate::backup::{refresh_backup, restore_from_backup};
use crate::config::Config;
use crate::daemon::should_shutdown;
use anyhow::{Context, Result};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Core event loop: watches the save directory, keeps a rolling backup fresh,
/// and restores it when the game wipes the saves (death).
pub fn run(config: Config) -> Result<()> {
    config.ensure_dirs()?;

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = notify::recommended_watcher(tx)
        .context("Failed to create filesystem watcher")?;
    register_watches(&mut watcher, &config)?;

    log::info!("Watching {} for save changes", config.save_dir.display());
    log::info!("Backups stored in {}", config.backup_dir.display());

    // Take an initial backup so a death right after start is protected.
    refresh_backup(&config);

    let mut last_save = Instant::now();
    let mut pending_restore_at: Option<Instant> = None;

    while !should_shutdown() {
        let event = match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(Ok(ev)) => ev,
            Ok(Err(e)) => {
                log::warn!("Watcher error: {e:?}");
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Heartbeat. Finish a pending restore once the debounce
                // window passes without the game rewriting its saves.
                if pending_restore_at
                    .map(|at| at.elapsed() >= Duration::from_millis(config.debounce_ms))
                    .unwrap_or(false)
                {
                    pending_restore_at = None;
                    last_save = Instant::now();
                    do_restore(&mut watcher, &config);
                }
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };

        if event.need_rescan() {
            log::warn!("inotify overflow; rescanned state");
            if config.save_dir.join("saves").is_dir() {
                refresh_backup(&config);
            }
        }

        match &event.kind {
            EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Data(_)) => {
                // A save write is in progress: cancel any pending restore and
                // refresh the rolling backup.
                if is_tracked_any(&event.paths, &config) {
                    pending_restore_at = None;
                    last_save = Instant::now();
                    refresh_backup(&config);
                }
            }
            EventKind::Remove(_) => {
                if pending_restore_at.is_some() {
                    continue;
                }
                let elapsed = last_save.elapsed();
                if elapsed > Duration::from_millis(config.debounce_ms) {
                    log::warn!(
                        "Save deletion detected ({} after last write) -> restoring",
                        format_duration(elapsed)
                    );
                    last_save = Instant::now();
                    do_restore(&mut watcher, &config);
                } else {
                    log::info!(
                        "Save deletion {} after last write; watching to see if the game recreates it",
                        format_duration(elapsed)
                    );
                    pending_restore_at = Some(Instant::now());
                }
            }
            _ => {}
        }
    }

    log::info!("Shutting down");
    Ok(())
}

/// Register inotify watches on the Repo root (non-recursive, to catch the
/// `saves/` folder itself being deleted) and on `saves/` (recursive).
fn register_watches(watcher: &mut dyn Watcher, config: &Config) -> Result<()> {
    watcher
        .watch(&config.save_dir, RecursiveMode::NonRecursive)
        .with_context(|| format!("Failed to watch {}", config.save_dir.display()))?;

    let saves_dir = config.save_dir.join("saves");
    if !saves_dir.exists() {
        std::fs::create_dir_all(&saves_dir)
            .with_context(|| format!("Failed to create {}", saves_dir.display()))?;
    }
    watcher
        .watch(&saves_dir, RecursiveMode::Recursive)
        .with_context(|| format!("Failed to watch {}", saves_dir.display()))?;
    Ok(())
}

/// Re-register the `saves/` watch after a restore (the old inode watch died
/// when the game deleted the directory).
fn rewatch_saves(watcher: &mut dyn Watcher, config: &Config) {
    let saves_dir = config.save_dir.join("saves");
    if let Err(e) = watcher.watch(&saves_dir, RecursiveMode::Recursive) {
        log::warn!("Failed to re-watch {}: {e}", saves_dir.display());
    }
}

fn do_restore(watcher: &mut dyn Watcher, config: &Config) {
    match restore_from_backup(config, None) {
        Ok(()) => {
            log::info!("Save restore complete");
            // The saves/ dir was recreated under a new inode; re-watch it and
            // drop any events the restore's own I/O queued up.
            rewatch_saves(watcher, config);
        }
        Err(e) => log::error!("Restore failed: {e:#}"),
    }
}

fn is_tracked_any(paths: &[std::path::PathBuf], config: &Config) -> bool {
    paths.iter().any(|p| {
        p.starts_with(&config.save_dir)
            && crate::tracked::is_tracked(p.as_path())
    })
}

fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    format!("{ms} ms")
}