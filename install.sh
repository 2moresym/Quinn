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
SERVICE_DIR="${HOME}/.config/systemd/user"
EXT_UUID="quinn@2moresym.org"
EXT_DIR="${HOME}/.local/share/gnome-shell/extensions/${EXT_UUID}"

VOSK_VERSION="0.3.45"
VOSK_ARCHIVE="vosk-linux-x86_64-${VOSK_VERSION}.zip"
VOSK_URL="https://github.com/alphacep/vosk-api/releases/download/v${VOSK_VERSION}/${VOSK_ARCHIVE}"
VOSK_MODEL_NAME="vosk-model-small-en-us-0.15"
VOSK_MODEL_ARCHIVE="${VOSK_MODEL_NAME}.zip"
VOSK_MODEL_URL="https://alphacephei.com/vosk/models/${VOSK_MODEL_ARCHIVE}"

mkdir -p "$MODEL_DIR" "$VOICE_DIR" "$VOSK_DIR" "$BIN_DIR" "$APP_DIR"

echo "== Quinn app preflight =="

command -v cargo >/dev/null 2>&1 || {
    echo "error: cargo is required to build Quinn." >&2
    exit 1
}
echo "cargo: ok"

if [[ "$(uname -m)" != "x86_64" ]]; then
    echo "error: bundled Vosk support currently targets x86_64 Linux." >&2
    exit 1
fi
echo "architecture: x86_64"

command -v pkg-config >/dev/null 2>&1 || {
    echo "error: pkg-config is required." >&2
    exit 1
}
pkg-config --exists alsa || {
    echo "error: ALSA development files are required. Install with: sudo apt install pkg-config libasound2-dev" >&2
    exit 1
}
pkg-config --exists gtk4 || {
    echo "error: GTK4 development files are required. Install with: sudo apt install libgtk-4-dev" >&2
    exit 1
}
echo "native build dependencies: ok"

if [[ ! -f "$MODEL_DIR/needle2.cact" ]]; then
    if ! command -v hf >/dev/null 2>&1; then
        echo "error: the Hugging Face 'hf' CLI is required only for the first Needle v2 download." >&2
        echo "Install the standalone hf CLI, then rerun ./install.sh." >&2
        exit 1
    fi
    echo "Downloading Needle v2 model..."
    hf download Cactus-Compute/needle2 needle2.cact --local-dir "$MODEL_DIR"
else
    echo "Needle v2 model: already installed; reusing it."
fi

if [[ ! -d "$ROOT/Voices/female" ]]; then
    echo "error: missing Voices/female voice pack." >&2
    exit 1
fi
cp -a "$ROOT/Voices/female/." "$VOICE_DIR/"

if [[ ! -f "$VOSK_DIR/libvosk.so" ]]; then
    command -v curl >/dev/null 2>&1 || { echo "error: curl is required to download Vosk." >&2; exit 1; }
    command -v unzip >/dev/null 2>&1 || { echo "error: unzip is required to install Vosk." >&2; exit 1; }
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT
    echo "Downloading Vosk native library..."
    curl -L --fail --retry 3 "$VOSK_URL" -o "$tmp_dir/$VOSK_ARCHIVE"
    unzip -q "$tmp_dir/$VOSK_ARCHIVE" -d "$tmp_dir"
    install -m 0644 "$tmp_dir/vosk-linux-x86_64-${VOSK_VERSION}/libvosk.so" "$VOSK_DIR/libvosk.so"
    rm -rf "$tmp_dir"
    trap - EXIT
else
    echo "Vosk native library: already installed; reusing it."
fi

if [[ ! -f "$VOSK_MODEL_DIR/am/final.mdl" ]]; then
    command -v curl >/dev/null 2>&1 || { echo "error: curl is required to download the Vosk model." >&2; exit 1; }
    command -v unzip >/dev/null 2>&1 || { echo "error: unzip is required to install the Vosk model." >&2; exit 1; }
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
    echo "Vosk speech model: already installed; reusing it."
fi

export QUINN_VOSK_LIB_DIR="$VOSK_DIR"
export QUINN_STT_MODEL="$VOSK_MODEL_DIR"

echo "Building Quinn daemon with voice support..."
cargo build --release --manifest-path "$ROOT/Cargo.toml" -p quinn-daemon --features voice

echo "Building standalone Quinn app..."
cargo build --release --manifest-path "$ROOT/Cargo.toml" -p quinn-app

install -m 0755 "$ROOT/target/release/quinn-daemon" "$BIN_DIR/quinn-daemon"
install -m 0755 "$ROOT/target/release/quinn-app" "$BIN_DIR/quinn-app"

# Remove the old extension/service architecture when upgrading from a previous Quinn build.
if command -v gnome-extensions >/dev/null 2>&1; then
    gnome-extensions disable "$EXT_UUID" >/dev/null 2>&1 || true
fi
rm -rf "$EXT_DIR"
systemctl --user disable --now quinn.service >/dev/null 2>&1 || true
rm -f "$SERVICE_DIR/quinn.service"
systemctl --user daemon-reload >/dev/null 2>&1 || true

cp "$ROOT/quinn-app/org.quinn.AssistantApp.desktop" "$APP_DIR/org.quinn.AssistantApp.desktop"
sed -i "s|^Exec=.*$|Exec=$BIN_DIR/quinn-app|" "$APP_DIR/org.quinn.AssistantApp.desktop"

echo
echo "Quinn app installed successfully."
echo "Launch it from GNOME Activities or run:"
echo "  $BIN_DIR/quinn-app"
