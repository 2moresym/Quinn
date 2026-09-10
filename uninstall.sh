#!/usr/bin/env bash
set -euo pipefail

BIN_DIR="${HOME}/.local/bin"
DATA_DIR="${HOME}/.local/share/quinn"
EXT_DIR="${HOME}/.local/share/gnome-shell/extensions/quinn@2moresym.org"
SERVICE_DIR="${HOME}/.config/systemd/user"

printf 'Stopping Quinn service...\n'
systemctl --user disable --now quinn.service 2>/dev/null || true
systemctl --user daemon-reload 2>/dev/null || true

printf 'Removing Quinn daemon...\n'
rm -f "$BIN_DIR/quinn-daemon"

printf 'Removing Quinn systemd unit...\n'
rm -f "$SERVICE_DIR/quinn.service"
systemctl --user daemon-reload 2>/dev/null || true

printf 'Removing Quinn GNOME extension...\n'
rm -rf "$EXT_DIR"

printf 'Removing Quinn data, model, and installed voices...\n'
rm -rf "$DATA_DIR"

if command -v gnome-extensions >/dev/null 2>&1; then
    gnome-extensions disable quinn@2moresym.org 2>/dev/null || true
fi

echo
echo 'Quinn has been uninstalled.'
echo 'Your Quinn source repository and Cargo cache were left untouched.'
