use crate::config::Config;
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

/// Copy a file with a few retries (Wine/Proton can briefly hold file locks).
fn copy_with_retry(src: &Path, dst: &Path, attempts: usize) -> Result<()> {
    let mut last_err = None;
    for attempt in 0..attempts {
        match fs::copy(src, dst) {
            Ok(_) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                if attempt + 1 < attempts {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    }
    Err(anyhow::anyhow!(
        "Failed to copy {} to {} after {attempts} attempts: {:?}",
        src.display(),
        dst.display(),
        last_err
    ))
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)
        .with_context(|| format!("Failed to create dir {}", dst.display()))?;
    for entry in fs::read_dir(src).context("Failed to read dir")? {
        let entry = entry?;
        let entry_path = entry.path();
        let file_type = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry_path, &dest_path)?;
        } else if file_type.is_symlink() {
            let target = fs::read_link(&entry_path)?;
            std::os::unix::fs::symlink(&target, &dest_path)?;
        } else if file_type.is_file() {
            copy_with_retry(&entry_path, &dest_path, 3)?;
        }
    }
    Ok(())
}

/// Create a rolled backup of the current save state.
/// Returns the path of the backup created.
pub fn create_backup(config: &Config) -> Result<PathBuf> {
    config.ensure_dirs()?;

    let timestamp = chrono_now();
    let backup_path = config.backup_dir.join(format!("rolled-{timestamp}"));

    if backup_path.exists() {
        fs::remove_dir_all(&backup_path)?;
    }
    fs::create_dir_all(&backup_path)?;

    for name in crate::tracked::TRACKED_FILES {
        let src = config.save_dir.join(name);
        if src.is_file() {
            copy_with_retry(&src, &backup_path.join(name), 3)?;
        }
    }

    let saves_src = config.save_dir.join("saves");
    if saves_src.is_dir() {
        copy_dir_recursive(&saves_src, &backup_path.join("saves"))?;
    }

    remove_old_rolled_backups(config);

    Ok(backup_path)
}

/// Remove old rolled backups, keeping only the configured number.
fn remove_old_rolled_backups(config: &Config) {
    let Ok(entries) = fs::read_dir(&config.backup_dir) else { return };
    let mut rolled: Vec<PathBuf> = entries
        .filter_map(|e| {
            let e = e.ok()?;
            if !e.path().is_dir() {
                return None;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("rolled-") || name.starts_with("pre-restore-") {
                Some(e.path())
            } else {
                None
            }
        })
        .collect();
    if rolled.len() <= 1 {
        return;
    }
    rolled.sort();
    while rolled.len() > 1 {
        let oldest = rolled.remove(0);
        let _ = fs::remove_dir_all(&oldest);
        log::debug!("Removed old rolling backup {}", oldest.display());
    }
}

/// Remove old rolling backups, keeping only the most recent one.
pub fn prune_old_backups(config: &Config) {
    let Ok(entries) = fs::read_dir(&config.backup_dir) else { return };
    let mut rolled: Vec<PathBuf> = entries
        .filter_map(|e| {
            let e = e.ok()?;
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && e.file_name().to_string_lossy().starts_with("rolled-")
            {
                Some(e.path())
            } else {
                None
            }
        })
        .collect();
    rolled.sort();
    while rolled.len() > 1 {
        let oldest = rolled.remove(0);
        if fs::remove_dir_all(&oldest).is_ok() {
            log::info!("Pruned old rolling backup {}", oldest.display());
        }
    }
}

/// Babysit the rolling backup: whenever the game writes a save while the
/// daemon is running, keep the rolling backup fresh. Called on change
/// events that signal a save write. Throttled to avoid thrashing during
/// multi-file save bursts.
pub fn refresh_backup(config: &Config) {
    use std::sync::OnceLock;
    use std::sync::Mutex;
    static LAST: OnceLock<Mutex<std::time::Instant>> = OnceLock::new();
    let last = LAST.get_or_init(|| Mutex::new(std::time::Instant::now() - std::time::Duration::from_secs(10)));

    {
        let Ok(guard) = last.lock() else { return };
        if guard.elapsed() < std::time::Duration::from_secs(1)
            && find_latest_backup(config).ok().flatten().is_some()
        {
            return;
        }
    }

    match create_backup(config) {
        Ok(path) => {
            if let Ok(mut guard) = last.lock() {
                *guard = std::time::Instant::now();
            }
            log::debug!("Backup refreshed at {}", path.display());
        }
        Err(e) => log::warn!("Backup refresh failed: {e:#}"),
    }
}

/// Find the most recent rolling backup.
pub fn find_latest_backup(config: &Config) -> Result<Option<PathBuf>> {
    if !config.backup_dir.is_dir() {
        return Ok(None);
    }
    let mut candidates: Vec<PathBuf> = fs::read_dir(&config.backup_dir)?
        .filter_map(|e| {
            let e = e.ok()?;
            let name = e.file_name().to_string_lossy().into_owned();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) && name.starts_with("rolled-") {
                Some(e.path())
            } else {
                None
            }
        })
        .collect();
    candidates.sort();
    Ok(candidates.pop())
}

/// Restore the save state from the latest rolling backup.
/// If `backup_path` is None, the most recent rolling backup is used.
pub fn restore_from_backup(config: &Config, backup_path: Option<&Path>) -> Result<()> {
    let backup_path = match backup_path {
        Some(p) => Some(p.to_path_buf()),
        None => find_latest_backup(config)?,
    };
    let backup_path = match backup_path {
        Some(p) => p,
        None => anyhow::bail!("No backup found in {}", config.backup_dir.display()),
    };
    if !backup_path.is_dir() {
        bail!("Backup dir {} does not exist", backup_path.display());
    }

    log::warn!(
        "RESTORING saves from {} -> {}",
        backup_path.display(),
        config.save_dir.display()
    );

    let pre_restore = config.backup_dir.join(format!(
        "pre-restore-{}",
        chrono_now()
    ));
    if let Err(e) = create_backup_into(config, &pre_restore) {
        log::warn!("Failed to snapshot pre-restore state: {e:#}");
    }

    fs::create_dir_all(&config.save_dir)
        .with_context(|| format!("Failed to create save dir {}", config.save_dir.display()))?;

    // Remove existing tracked state at the destination so restore is clean.
    for dir in crate::tracked::TRACKED_DIRS {
        let target = config.save_dir.join(dir);
        if let Ok(true) = fs::symlink_metadata(&target).map(|m| m.is_dir()) {
            let _ = fs::remove_dir_all(&target);
        }
    }
    for file in crate::tracked::TRACKED_FILES {
        let target = config.save_dir.join(file);
        let _ = fs::remove_file(&target);
    }

    for dir in crate::tracked::TRACKED_DIRS {
        let src = backup_path.join(dir);
        if src.is_dir() {
            copy_dir_recursive(&src, &config.save_dir.join(dir))?;
        }
    }
    for file in crate::tracked::TRACKED_FILES {
        let src = backup_path.join(file);
        if src.is_file() {
            copy_with_retry(&src, &config.save_dir.join(file), 3)?;
        }
    }

    // Also copy any other files the game stores directly in the Repo root.
    if let Ok(entries) = fs::read_dir(&backup_path) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str == "saves"
                || crate::tracked::TRACKED_FILES.contains(&name_str.as_ref())
                || name_str.starts_with("pre-restore-")
            {
                continue;
            }
            let dst = config.save_dir.join(&name);
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                copy_dir_recursive(&entry.path(), &dst)?;
            } else if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                copy_with_retry(&entry.path(), &dst, 3)?;
            }
        }
    }

    log::info!("Restore complete.");
    Ok(())
}

/// Create a backup directly into a specific target directory.
fn create_backup_into(config: &Config, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for name in crate::tracked::TRACKED_FILES {
        let src = config.save_dir.join(name);
        if src.is_file() {
            copy_with_retry(&src, &target.join(name), 3)?;
        }
    }
    let saves_src = config.save_dir.join("saves");
    if saves_src.is_dir() {
        copy_dir_recursive(&saves_src, &target.join("saves"))?;
    }
    Ok(())
}

/// List all backup directories (rolling + manual) in the backup dir.
pub fn list_backups(config: &Config) -> Result<Vec<PathBuf>> {
    if !config.backup_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut backups: Vec<PathBuf> = fs::read_dir(&config.backup_dir)?
        .filter_map(|e| {
            let e = e.ok()?;
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                Some(e.path())
            } else {
                None
            }
        })
        .collect();
    backups.sort();
    Ok(backups)
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs();
    let days = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{days:05}_{h:02}_{m:02}_{s:02}")
}