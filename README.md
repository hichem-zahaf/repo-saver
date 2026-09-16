# repo-saver

Data-loss protector for the game **[REPO](https://store.steampowered.com/app/3241660)** (Steam App ID `3241660`).

When a player dies in REPO, the multiplayer host's save can be wiped by the
game. `repo-saver` watches the save directory, maintains a rolling backup in
the background, and automatically restores your saves when the game deletes
them on death.

## Features

- **Live protection** – watches the REPO save folder and instantly restores
  backed-up state when the game wipes it (with a configurable debounce window).
- **Rolling backup** – keeps a fresh copy of the latest save whenever the game
  writes one.
- **GUI** – start/stop protection, take/restore backups, and watch activity,
  all from a window.
- **Wine/Proton friendly** – file copies retry around transient Wine/Proton
  file locks.

## Build

```bash
cargo build --release
```

Requires Rust (edition 2021). The binary is a desktop GUI built on
[`egui`/`eframe`](https://github.com/emilk/egui).

## Usage

```bash
cargo run --release
# or
./target/release/repo-saver
```

In the window:

- **Start monitor** – begins watching the save directory and auto-restoring
  on death. Settings (save dir, backup dir, debounce) are locked while the
  monitor is running.
- **Stop monitor** – stops watching and closes the protection.
- **Backup now** – manually snapshot the current save state.
- **Restore latest** – restore the most recent rolling backup.
- **Backups** – list rolling backups and restore any specific one.
- **Activity log** – shows live events (watched dir, restores, errors).

> Restore while the game is not running for best results; the game may hold
> its own state in memory otherwise.

## Default paths

| Thing | Path |
| --- | --- |
| Save dir | `~/.local/share/Steam/steamapps/compatdata/3241660/pfx/drive_c/users/steamuser/AppData/LocalLow/semiwork/Repo` |
| Backup dir | `~/.local/share/repo-saver/backups` |

Both directories and the debounce window (default `3000` ms) can be overridden
in the GUI, or via the `REPO_SAVE_DIR` / `REPO_BACKUP_DIR` environment
variables.

`MetaSave.es3`, `MetaSave.bak`, `SettingsData.es3` and the `saves/` folder are
the tracked state.