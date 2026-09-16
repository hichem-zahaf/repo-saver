use anyhow::{Context, Result, bail};
use std::env;
use std::path::PathBuf;

const DEFAULT_SAVE_DIR: &str = ".local/share/Steam/steamapps/compatdata/3241660/pfx/drive_c/users/steamuser/AppData/LocalLow/semiwork/Repo";

#[derive(Clone, Debug)]
pub struct Config {
    pub save_dir: PathBuf,
    pub backup_dir: PathBuf,
    pub debounce_ms: u64,
}

impl Config {
    pub fn new(save_dir_override: Option<PathBuf>, backup_dir_override: Option<PathBuf>, debounce_ms: u64) -> Result<Self> {
        let home = dirs::home_dir().context("Failed to determine home directory")?;

        let save_dir = save_dir_override
            .or_else(|| env::var_os("REPO_SAVE_DIR").map(PathBuf::from))
            .unwrap_or_else(|| home.join(DEFAULT_SAVE_DIR));

        let backup_dir = backup_dir_override
            .or_else(|| env::var_os("REPO_BACKUP_DIR").map(PathBuf::from))
            .unwrap_or_else(|| home.join(".local/share/repo-saver/backups"));

        if backup_dir == save_dir {
            bail!("Backup directory cannot be the same as save directory");
        }

        Ok(Config {
            save_dir,
            backup_dir,
            debounce_ms,
        })
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.save_dir)
            .with_context(|| format!("Failed to create save dir {}", self.save_dir.display()))?;
        std::fs::create_dir_all(&self.backup_dir)
            .with_context(|| format!("Failed to create backup dir {}", self.backup_dir.display()))?;
        Ok(())
    }
}