#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
DATA_DIR="${HOME}/.local/share/quinn"
MODEL_DIR="${DATA_DIR}/weights"
VOICE_DIR="${DATA_DIR}/voices/female"
VOSK_DIR="${DATA_DIR}/vosk"
VOSK_MODEL_DIR="${DATA_DIR}/voice-model"
BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"

VOSK_VERSION="0.3.45"
VOSK_ARCHIVE="vosk-linux-x86_64-${VOSK_VERSION}.zip"
VOSK_URL="https://github.com/alphacep/vosk-api/releases/download/v${VOSK_VERSION}/${VOSK_ARCHIVE}"
VOSK_MODEL_NAME="vosk-model-small-en-us-0.15"
VOSK_MODEL_ARCHIVE="${VOSK_MODEL_NAME}.zip"
VOSK_MODEL_URL="https://alphacephei.com/vosk/models/${VOSK_MODEL_ARCHIVE}"

mkdir -p "$MODEL_DIR" "$VOICE_DIR" "$VOSK_DIR" "$BIN_DIR" "$APP_DIR" "$ICON_DIR"

echo "== Quinn Beta installer =="
command -v pkg-config >/dev/null 2>&1 || { echo "error: pkg-config is required." >&2; exit 1; }
pkg-config --exists alsa || { echo "error: install pkg-config libasound2-dev." >&2; exit 1; }
pkg-config --exists gtk4 || { echo "error: install libgtk-4-dev." >&2; exit 1; }
[[ "$(uname -m)" == "x86_64" ]] || { echo "error: bundled Vosk supports x86_64 Linux." >&2; exit 1; }
command -v curl >/dev/null 2>&1 || { echo "error: curl is required." >&2; exit 1; }
command -v unzip >/dev/null 2>&1 || { echo "error: unzip is required." >&2; exit 1; }

echo "Native dependencies: ok"

if [[ ! -f "$MODEL_DIR/needle2.cact" ]]; then
    if [[ -f "$ROOT/needle2.cact" ]]; then
        echo "Installing bundled Needle v2 model..."
        install -m 0644 "$ROOT/needle2.cact" "$MODEL_DIR/needle2.cact"
    else
        command -v hf >/dev/null 2>&1 || { echo "error: install the standalone hf CLI for the first Needle download." >&2; exit 1; }
        echo "Downloading Needle v2 model..."
        hf download Cactus-Compute/needle2 needle2.cact --local-dir "$MODEL_DIR"
    fi
else
    echo "Needle v2: reusing existing model"
fi

if [[ ! -f "$VOSK_DIR/libvosk.so" ]]; then
    if [[ -f "$ROOT/libvosk.so" ]]; then
        echo "Installing bundled Vosk native library..."
        install -m 0644 "$ROOT/libvosk.so" "$VOSK_DIR/libvosk.so"
    else
        tmp_dir=$(mktemp -d)
        trap 'rm -rf "$tmp_dir"' EXIT
        echo "Downloading Vosk native library..."
        curl -L --fail --retry 3 "$VOSK_URL" -o "$tmp_dir/$VOSK_ARCHIVE"
        unzip -q "$tmp_dir/$VOSK_ARCHIVE" -d "$tmp_dir"
        install -m 0644 "$tmp_dir/vosk-linux-x86_64-${VOSK_VERSION}/libvosk.so" "$VOSK_DIR/libvosk.so"
        rm -rf "$tmp_dir"
        trap - EXIT
    fi
else
    echo "Vosk native library: reusing existing copy"
fi

if [[ ! -f "$VOSK_MODEL_DIR/am/final.mdl" ]]; then
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT
    echo "Downloading lightweight Vosk English speech model (~40 MB)..."
    curl -L --fail --retry 3 "$VOSK_MODEL_URL" -o "$tmp_dir/$VOSK_MODEL_ARCHIVE"
    unzip -q "$tmp_dir/$VOSK_MODEL_ARCHIVE" -d "$tmp_dir"
    rm -rf "$VOSK_MODEL_DIR"
    mv "$tmp_dir/$VOSK_MODEL_NAME" "$VOSK_MODEL_DIR"
    rm -rf "$tmp_dir"
    trap - EXIT
else
    echo "Vosk speech model: reusing existing copy"
fi

if [[ ! -d "$ROOT/Voices/female" ]]; then
    echo "error: Voices/female is missing." >&2
    exit 1
fi
cp -a "$ROOT/Voices/female/." "$VOICE_DIR/"

if [[ -x "$ROOT/quinn-daemon" ]]; then
    install -m 0755 "$ROOT/quinn-daemon" "$BIN_DIR/quinn-daemon"
else
    export QUINN_VOSK_LIB_DIR="$VOSK_DIR"
    export QUINN_STT_MODEL="$VOSK_MODEL_DIR"
    command -v cargo >/dev/null 2>&1 || { echo "error: cargo is required when installing from source." >&2; exit 1; }
    echo "Building Quinn daemon..."
    cargo build --release -p quinn-daemon --features voice
    install -m 0755 target/release/quinn-daemon "$BIN_DIR/quinn-daemon"
fi

if [[ -x "$ROOT/quinn-app" ]]; then
    install -m 0755 "$ROOT/quinn-app" "$BIN_DIR/quinn-app"
else
    command -v cargo >/dev/null 2>&1 || { echo "error: cargo is required when installing from source." >&2; exit 1; }
    echo "Building Quinn app..."
    cargo build --release -p quinn-app
    install -m 0755 target/release/quinn-app "$BIN_DIR/quinn-app"
fi

if [[ -x "$ROOT/quinn-updater" ]]; then
    install -m 0755 "$ROOT/quinn-updater" "$BIN_DIR/quinn-updater"
elif command -v cargo >/dev/null 2>&1; then
    echo "Building Quinn updater..."
    cargo build --release -p quinn-updater
    install -m 0755 target/release/quinn-updater "$BIN_DIR/quinn-updater"
fi

if [[ -f "$ROOT/quinn.svg" ]]; then
    install -m 0644 "$ROOT/quinn.svg" "$ICON_DIR/quinn.svg"
elif [[ -f "$ROOT/Icon/Quinn_Icon.svg" ]]; then
    install -m 0644 "$ROOT/Icon/Quinn_Icon.svg" "$ICON_DIR/quinn.svg"
fi

cp "$ROOT/quinn-app/org.quinn.AssistantApp.desktop" "$APP_DIR/org.quinn.AssistantApp.desktop"
sed -i "s|^Exec=.*$|Exec=$BIN_DIR/quinn-app|" "$APP_DIR/org.quinn.AssistantApp.desktop"

# Remove legacy GNOME extension/service installations.
if command -v gnome-extensions >/dev/null 2>&1; then
    gnome-extensions disable quinn@2moresym.org >/dev/null 2>&1 || true
fi
rm -rf "$HOME/.local/share/gnome-shell/extensions/quinn@2moresym.org"
systemctl --user disable --now quinn.service >/dev/null 2>&1 || true
rm -f "$HOME/.config/systemd/user/quinn.service"
systemctl --user daemon-reload >/dev/null 2>&1 || true

glib-compile-schemas /usr/share/glib-2.0/schemas >/dev/null 2>&1 || true

printf '\nQuinn Beta installed.\nLaunch: %s\n' "$BIN_DIR/quinn-app"
