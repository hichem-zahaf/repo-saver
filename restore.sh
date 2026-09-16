#!/bin/bash

SAVE_DIR="$HOME/.local/share/Steam/steamapps/compatdata/3241660/pfx/drive_c/users/steamuser/AppData/LocalLow/semiwork/Repo"
BACKUP_DIR="$HOME/Projects/repo-saver/backups"

LATEST=$(ls -td "$BACKUP_DIR"/*/ 2>/dev/null | head -1)

if [ -z "$LATEST" ]; then
    echo "No backups found."
    exit 1
fi

echo "Restoring from: $LATEST"
cp "$LATEST/MetaSave.es3" "$SAVE_DIR/"
cp "$LATEST/MetaSave.bak" "$SAVE_DIR/"
cp "$LATEST/SettingsData.es3" "$SAVE_DIR/"
rm -rf "$SAVE_DIR/saves"
cp -r "$LATEST/saves" "$SAVE_DIR/"

echo "Restore complete."
