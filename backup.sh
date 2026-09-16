#!/bin/bash

SAVE_DIR="$HOME/.local/share/Steam/steamapps/compatdata/3241660/pfx/drive_c/users/steamuser/AppData/LocalLow/semiwork/Repo"
BACKUP_DIR="$HOME/Projects/repo-saver/backups"
TIMESTAMP=$(date +%Y_%m_%d_%H_%M_%S)
DEST="$BACKUP_DIR/$TIMESTAMP"

mkdir -p "$DEST"
cp "$SAVE_DIR/MetaSave.es3" "$DEST/"
cp "$SAVE_DIR/MetaSave.bak" "$DEST/"
cp "$SAVE_DIR/SettingsData.es3" "$DEST/"
cp -r "$SAVE_DIR/saves" "$DEST/"

echo "Backup created at $DEST"
