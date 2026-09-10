#!/usr/bin/env bash
set -euo pipefail

BIN_DIR="${HOME}/.local/bin"
DATA_DIR="${HOME}/.local/share/quinn"
APP_DIR="${HOME}/.local/share/applications"
EXT_DIR="${HOME}/.local/share/gnome-shell/extensions/quinn@2moresym.org"
SERVICE_DIR="${HOME}/.config/systemd/user"

printf 'Stopping legacy Quinn service...\n'
systemctl --user disable --now quinn.service 2>/dev/null || true
systemctl --user daemon-reload 2>/dev/null || true

printf 'Removing Quinn applications...\n'
rm -f "$BIN_DIR/quinn-daemon" "$BIN_DIR/quinn-app"
rm -f "$APP_DIR/org.quinn.AssistantApp.desktop"

printf 'Removing legacy Quinn systemd unit and GNOME extension...\n'
rm -f "$SERVICE_DIR/quinn.service"
rm -rf "$EXT_DIR"
systemctl --user daemon-reload 2>/dev/null || true

gnome_settings=$(gsettings get org.gnome.shell enabled-extensions 2>/dev/null || true)
if command -v gnome-extensions >/dev/null 2>&1; then
    gnome-extensions disable quinn@2moresym.org 2>/dev/null || true
fi

printf 'Removing Quinn models and installed voices...\n'
rm -rf "$DATA_DIR"

echo
echo 'Quinn has been uninstalled.'
echo 'Your Quinn source repository and Cargo cache were left untouched.'
