pub const TRACKED_FILES: &[&str] = &["MetaSave.es3", "MetaSave.bak", "SettingsData.es3"];
pub const TRACKED_DIRS: &[&str] = &["saves"];
pub const DEBOUNCE_MS: u64 = 3000;

pub fn is_tracked(path: &std::path::Path) -> bool {
    if path.is_dir() {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| TRACKED_DIRS.contains(&n))
            .unwrap_or(false)
    } else {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| TRACKED_FILES.contains(&n))
            .unwrap_or(false)
    }
}