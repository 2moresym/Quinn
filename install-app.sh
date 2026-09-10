#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
MODEL_DIR="${HOME}/.local/share/quinn/weights"
VOICE_DIR="${HOME}/.local/share/quinn/voices/female"
VOSK_DIR="${HOME}/.local/share/quinn/vosk"
VOSK_MODEL_DIR="${HOME}/.local/share/quinn/voice-model"

mkdir -p "$BIN_DIR" "$APP_DIR" "$VOICE_DIR"

echo "== Quinn app preflight =="
command -v cargo >/dev/null 2>&1 || { echo "error: cargo is required." >&2; exit 1; }
command -v pkg-config >/dev/null 2>&1 || { echo "error: pkg-config is required." >&2; exit 1; }
pkg-config --exists gtk4 || { echo "error: GTK4 development files are required. Install with: sudo apt install libgtk-4-dev" >&2; exit 1; }
echo "GTK4 development files: ok"

arch="$(uname -m)"
if [[ "$arch" != "x86_64" ]]; then
    echo "error: bundled Vosk support currently targets x86_64 Linux." >&2
    exit 1
fi
echo "architecture: x86_64"

if [[ ! -f "$MODEL_DIR/needle2.cact" ]]; then
    echo "error: Needle v2 model is missing at $MODEL_DIR/needle2.cact. Run ./install.sh once to install Quinn's models." >&2
    exit 1
fi
if [[ ! -f "$VOSK_DIR/libvosk.so" ]]; then
    echo "error: native Vosk library is missing at $VOSK_DIR/libvosk.so. Run ./install.sh once." >&2
    exit 1
fi
if [[ ! -f "$VOSK_MODEL_DIR/am/final.mdl" ]]; then
    echo "error: Vosk speech model is missing at $VOSK_MODEL_DIR. Run ./install.sh once." >&2
    exit 1
fi
if [[ ! -x "$BIN_DIR/quinn-daemon" ]]; then
    echo "error: quinn-daemon is not installed at $BIN_DIR/quinn-daemon. Run ./install.sh once." >&2
    exit 1
fi

if [[ ! -d "$ROOT/Voices/female" ]]; then
    echo "error: missing Voices/female voice pack." >&2
    exit 1
fi
cp -a "$ROOT/Voices/female/." "$VOICE_DIR/"

echo "Building standalone Quinn app..."
cargo build --release --manifest-path "$ROOT/Cargo.toml" -p quinn-app
install -m 0755 "$ROOT/target/release/quinn-app" "$BIN_DIR/quinn-app"

cp "$ROOT/quinn-app/org.quinn.AssistantApp.desktop" "$APP_DIR/org.quinn.AssistantApp.desktop"
sed -i "s|^Exec=.*$|Exec=$BIN_DIR/quinn-app|" "$APP_DIR/org.quinn.AssistantApp.desktop"

echo
echo "Quinn app installed. Launch it from GNOME Activities or run:"
echo "  $BIN_DIR/quinn-app"
