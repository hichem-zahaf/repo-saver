#!/bin/bash

SAVE_DIR="$HOME/.local/share/Steam/steamapps/compatdata/3241660/pfx/drive_c/users/steamuser/AppData/LocalLow/semiwork/Repo"

rm -f "$SAVE_DIR/MetaSave.es3"
rm -f "$SAVE_DIR/MetaSave.bak"
rm -f "$SAVE_DIR/SettingsData.es3"
rm -rf "$SAVE_DIR/saves"

echo "Saves deleted."
