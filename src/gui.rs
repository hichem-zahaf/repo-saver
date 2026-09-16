use crate::backup;
use crate::config::Config;
use crate::{daemon, watcher};
use eframe::egui;
use egui::{Color32, RichText, ScrollArea};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

const MAX_LOG_LINES: usize = 800;

#[derive(Default)]
struct LogBuffer {
    lines: Mutex<VecDeque<String>>,
}

impl LogBuffer {
    fn push(&self, line: String) {
        if let Ok(mut lines) = self.lines.lock() {
            lines.push_back(line);
            while lines.len() > MAX_LOG_LINES {
                lines.pop_front();
            }
        }
    }

    fn snapshot(&self) -> Vec<String> {
        self.lines
            .lock()
            .map(|l| l.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn clear(&self) {
        if let Ok(mut lines) = self.lines.lock() {
            lines.clear();
        }
    }
}

impl log::Log for LogBuffer {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() >= log::Level::Warn
            || metadata.target().starts_with("repo_saver")
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!("[{}] {}", record.level(), record.args());
        self.push(line.clone());
        eprintln!("{line}");
    }

    fn flush(&self) {}
}

static LOG: OnceLock<Arc<LogBuffer>> = OnceLock::new();

/// Route `log` crate output into an in-memory buffer that the GUI renders.
pub fn init_logger() {
    let buf: &'static Arc<LogBuffer> = LOG.get_or_init(|| Arc::new(LogBuffer::default()));
    if log::set_logger(&**buf).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

fn log_line(line: impl Into<String>) {
    if let Some(buf) = LOG.get() {
        buf.push(line.into());
    }
}

fn log_snapshot() -> Vec<String> {
    LOG.get().map(|b| b.snapshot()).unwrap_or_default()
}

fn log_clear() {
    if let Some(buf) = LOG.get() {
        buf.clear();
    }
}

pub struct RepoGuiApp {
    save_dir: String,
    backup_dir: String,
    debounce_ms: String,
    running: bool,
    monitor: Option<JoinHandle<anyhow::Result<()>>>,
    backups: Vec<PathBuf>,
}

impl Default for RepoGuiApp {
    fn default() -> Self {
        let cfg = Config::new(None, None, crate::tracked::DEBOUNCE_MS).ok();
        let (save_dir, backup_dir, debounce_ms) = match cfg {
            Some(c) => (
                c.save_dir.to_string_lossy().into_owned(),
                c.backup_dir.to_string_lossy().into_owned(),
                c.debounce_ms.to_string(),
            ),
            None => (
                ".local/share/Steam/steamapps/compatdata/3241660/pfx/drive_c/users/steamuser/AppData/LocalLow/semiwork/Repo".to_string(),
                ".local/share/repo-saver/backups".to_string(),
                crate::tracked::DEBOUNCE_MS.to_string(),
            ),
        };
        let mut app = Self {
            save_dir,
            backup_dir,
            debounce_ms,
            running: false,
            monitor: None,
            backups: Vec::new(),
        };
        app.refresh_backups();
        app
    }
}

impl RepoGuiApp {
    fn build_config(&self) -> anyhow::Result<Config> {
        let save_dir = if self.save_dir.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(self.save_dir.trim()))
        };
        let backup_dir = if self.backup_dir.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(self.backup_dir.trim()))
        };
        let debounce_ms = self
            .debounce_ms
            .trim()
            .parse::<u64>()
            .map_err(|_| anyhow::anyhow!("Debounce must be a number of milliseconds"))?;
        Config::new(save_dir, backup_dir, debounce_ms)
    }

    fn start_monitor(&mut self) {
        if self.running {
            return;
        }
        match self.build_config() {
            Ok(cfg) => {
                if let Err(e) = cfg.ensure_dirs() {
                    log_line(format!("Failed to prepare directories: {e:#}"));
                    return;
                }
                daemon::reset();
                let cfg = cfg.clone();
                self.monitor = Some(std::thread::spawn(move || watcher::run(cfg)));
                self.running = true;
                log_line("Monitor started. REPO saves are now protected.");
                self.refresh_backups();
            }
            Err(e) => log_line(format!("Cannot start monitor: {e:#}")),
        }
    }

    fn stop_monitor(&mut self) {
        if !self.running {
            return;
        }
        daemon::request_stop();
        if let Some(handle) = self.monitor.take() {
            let _ = handle.join();
        }
        daemon::reset();
        self.running = false;
        log_line("Monitor stopped. Saves are no longer watched.");
        self.refresh_backups();
    }

    fn poll_monitor(&mut self) {
        if !self.running {
            return;
        }
        let finished = self
            .monitor
            .as_ref()
            .map(|h| h.is_finished())
            .unwrap_or(true);
        if finished {
            if let Some(handle) = self.monitor.take() {
                match handle.join() {
                    Ok(Ok(())) => log_line("Monitor ended cleanly."),
                    Ok(Err(e)) => log_line(format!("Monitor ended with error: {e:#}")),
                    Err(_) => log_line("Monitor thread panicked."),
                }
            }
            daemon::reset();
            self.running = false;
            self.refresh_backups();
        }
    }

    fn backup_now(&self) {
        match self.build_config() {
            Ok(cfg) => match backup::create_backup(&cfg) {
                Ok(path) => log_line(format!("Backup saved to {}", path.display())),
                Err(e) => log_line(format!("Backup failed: {e:#}")),
            },
            Err(e) => log_line(format!("Cannot back up: {e:#}")),
        }
    }

    fn restore_latest(&self) {
        match self.build_config() {
            Ok(cfg) => match backup::restore_from_backup(&cfg, None) {
                Ok(()) => log_line("Restored saves from the latest backup."),
                Err(e) => log_line(format!("Restore failed: {e:#}")),
            },
            Err(e) => log_line(format!("Cannot restore: {e:#}")),
        }
    }

    fn restore_backup(&self, path: &std::path::Path) {
        match self.build_config() {
            Ok(cfg) => match backup::restore_from_backup(&cfg, Some(path)) {
                Ok(()) => log_line(format!("Restored saves from {}", path.display())),
                Err(e) => log_line(format!("Restore failed: {e:#}")),
            },
            Err(e) => log_line(format!("Cannot restore: {e:#}")),
        }
    }

    fn refresh_backups(&mut self) {
        match self.build_config().and_then(|cfg| backup::list_backups(&cfg)) {
            Ok(list) => self.backups = list,
            Err(e) => {
                log_line(format!("Cannot list backups: {e:#}"));
                self.backups = Vec::new();
            }
        }
    }

    fn prune_old_backups(&self) {
        match self.build_config() {
            Ok(cfg) => {
                backup::prune_old_backups(&cfg);
            }
            Err(e) => log_line(format!("Cannot prune backups: {e:#}")),
        }
    }
}

impl eframe::App for RepoGuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_monitor();

        egui::Panel::top("status")
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let (color, text) = if self.running {
                        (Color32::from_rgb(90, 190, 120), "RUNNING")
                    } else {
                        (Color32::from_rgb(215, 110, 105), "STOPPED")
                    };
                    ui.label(RichText::new("STATUS:").strong());
                    ui.label(RichText::new(text).color(color).strong().size(18.0));

                    ui.add_space(16.0);
                    if self.running {
                        if ui.button("Stop monitor").clicked() {
                            self.stop_monitor();
                        }
                    } else if ui.button("Start monitor").clicked() {
                        self.start_monitor();
                    }

                    ui.add_space(8.0);
                    if ui.button("Refresh backups").clicked() {
                        self.refresh_backups();
                    }

                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.label(format!("{} backup(s)", self.backups.len()));
                        },
                    );
                });
                ui.add_space(8.0);
            });

        egui::CentralPanel::default()
            .show(ui, |ui| {
                ui.heading("Settings");
                ui.add_space(4.0);
                egui::Grid::new("settings")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Save directory");
                        ui.add_enabled_ui(!self.running, |ui| {
                            ui.text_edit_singleline(&mut self.save_dir);
                        });
                        ui.end_row();

                        ui.label("Backup directory");
                        ui.add_enabled_ui(!self.running, |ui| {
                            ui.text_edit_singleline(&mut self.backup_dir);
                        });
                        ui.end_row();

                        ui.label("Debounce (ms)");
                        ui.add_enabled_ui(!self.running, |ui| {
                            ui.text_edit_singleline(&mut self.debounce_ms);
                        });
                        ui.end_row();
                    });

                ui.add_space(12.0);
                ui.heading("Actions");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Backup now").clicked() {
                        self.backup_now();
                        self.refresh_backups();
                    }
                    if ui.button("Restore latest").clicked() {
                        self.restore_latest();
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Restoring while the game is running can overwrite the game's \
                         in-memory saves. Prefer restoring before/after a session.",
                    )
                    .weak()
                    .small(),
                );

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.heading("Backups");
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui.button("Prune old backups").clicked() {
                                self.prune_old_backups();
                                self.refresh_backups();
                            }
                        },
                    );
                });
                ui.add_space(4.0);
                ScrollArea::vertical()
                    .id_salt("backup_list")
                    .max_height(140.0)
                    .show(ui, |ui| {
                        if self.backups.is_empty() {
                            ui.label("No backups yet. Press \"Backup now\" or start the monitor.");
                        } else {
                            for path in self.backups.iter().rev() {
                                ui.horizontal(|ui| {
                                    ui.label(path.display().to_string());
                                    if ui.small_button("Restore").clicked() {
                                        self.restore_backup(path);
                                    }
                                });
                            }
                        }
                    });
                ui.add_space(8.0);
            });

        egui::Panel::bottom("log")
            .resizable(true)
            .default_size(180.0)
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.strong("Activity log");
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui.button("Clear").clicked() {
                                log_clear();
                            }
                        },
                    );
                });
                ScrollArea::vertical()
                    .id_salt("activity_log")
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in log_snapshot() {
                            ui.monospace(line);
                        }
                    });
            });
    }
}