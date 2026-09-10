#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
DATA_DIR="${HOME}/.local/share/quinn"
MODEL_DIR="${DATA_DIR}/weights"
VOICE_DIR="${DATA_DIR}/voices/female"
VOSK_DIR="${DATA_DIR}/vosk"
EXT_DIR="${HOME}/.local/share/gnome-shell/extensions/quinn@2moresym.org"
BIN_DIR="${HOME}/.local/bin"
SERVICE_DIR="${HOME}/.config/systemd/user"
VOSK_VERSION="0.3.45"
VOSK_ARCHIVE="vosk-linux-x86_64-${VOSK_VERSION}.zip"
VOSK_URL="https://github.com/alphacep/vosk-api/releases/download/v${VOSK_VERSION}/${VOSK_ARCHIVE}"

mkdir -p "$MODEL_DIR" "$VOICE_DIR" "$VOSK_DIR" "$BIN_DIR" "$SERVICE_DIR"

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo is required to build Quinn." >&2
    exit 1
fi

if [[ ! -f "$MODEL_DIR/needle2.cact" ]]; then
    if ! command -v hf >/dev/null 2>&1; then
        echo "error: the Hugging Face CLI 'hf' is required for the first Needle v2 model download." >&2
        echo "Install the standalone 'hf' CLI, then rerun ./install.sh." >&2
        exit 1
    fi
    echo "Downloading Needle v2 model..."
    hf download Cactus-Compute/needle2 needle2.cact --local-dir "$MODEL_DIR"
else
    echo "Needle v2 model already installed; reusing it."
fi

if [[ ! -d "$ROOT/Voices/female" ]]; then
    echo "error: missing Voices/female voice pack." >&2
    exit 1
fi

VOICE_FILES=(
    "Hmm.wav"
    "Good_Morning.wav"
    "Good_Afternoon.wav"
    "Good_Evening.wav"
    "Good_Night.wav"
    "Done.wav"
    "Sorry.wav"
    "Understood.wav"
    "Timer_Set.wav"
    "Timer_Removed.wav"
)
for voice_file in "${VOICE_FILES[@]}"; do
    if [[ ! -f "$ROOT/Voices/female/$voice_file" ]]; then
        echo "warning: missing optional voice clip: $voice_file" >&2
    fi
done

if [[ "$(uname -m)" != "x86_64" ]]; then
    echo "error: Quinn's bundled Vosk library currently supports x86_64 Linux only." >&2
    exit 1
fi

if [[ ! -f "$VOSK_DIR/libvosk.so" ]]; then
    if ! command -v curl >/dev/null 2>&1 || ! command -v unzip >/dev/null 2>&1; then
        echo "error: curl and unzip are required to install the native Vosk library." >&2
        exit 1
    fi
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT
    echo "Downloading Vosk native library..."
    curl -L --fail --retry 3 "$VOSK_URL" -o "$tmp_dir/$VOSK_ARCHIVE"
    unzip -q "$tmp_dir/$VOSK_ARCHIVE" -d "$tmp_dir"
    install -m 0644 "$tmp_dir/vosk-linux-x86_64-${VOSK_VERSION}/libvosk.so" "$VOSK_DIR/libvosk.so"
    rm -rf "$tmp_dir"
    trap - EXIT
else
    echo "Vosk native library already installed; reusing it."
fi

if ! command -v pkg-config >/dev/null 2>&1 || ! pkg-config --exists alsa; then
    echo "warning: ALSA development files were not detected. Install them with:" >&2
    echo "  sudo apt install pkg-config libasound2-dev" >&2
fi

export QUINN_VOSK_LIB_DIR="$VOSK_DIR"

echo "Building quinn-daemon with voice support..."
cargo build --release --manifest-path "$ROOT/Cargo.toml" -p quinn-daemon --features voice

if [[ ! -x "$ROOT/target/release/quinn-daemon" ]]; then
    echo "error: Quinn daemon build completed without producing target/release/quinn-daemon." >&2
    exit 1
fi

install -m 0755 "$ROOT/target/release/quinn-daemon" "$BIN_DIR/quinn-daemon"

echo "Installing Quinn voice clips..."
cp -a "$ROOT/Voices/female/." "$VOICE_DIR/"

rm -rf "$EXT_DIR"
mkdir -p "$EXT_DIR"
cp -a "$ROOT/extension/quinn@2moresym.org/." "$EXT_DIR/"
if command -v glib-compile-schemas >/dev/null 2>&1 && [[ -d "$EXT_DIR/schemas" ]]; then
    glib-compile-schemas "$EXT_DIR/schemas"
fi

cp "$ROOT/systemd/quinn.service" "$SERVICE_DIR/quinn.service"
systemctl --user daemon-reload
systemctl --user enable --now quinn.service

if command -v gnome-extensions >/dev/null 2>&1; then
    gnome-extensions enable quinn@2moresym.org || true
fi

echo
echo 'Quinn installed. Test the daemon with:'
echo '  busctl --user introspect org.quinn.Assistant /org/quinn/Assistant'
echo 'Then try:'
echo '  busctl --user call org.quinn.Assistant /org/quinn/Assistant org.quinn.Assistant Ask s "open terminal"'
