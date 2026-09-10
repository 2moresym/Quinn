#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
DATA_DIR="${HOME}/.local/share/quinn"
MODEL_DIR="${DATA_DIR}/weights"
VOICE_DIR="${DATA_DIR}/voices/female"
VOSK_DIR="${DATA_DIR}/vosk"
VOSK_MODEL_DIR="${DATA_DIR}/voice-model"
EXT_UUID="quinn@2moresym.org"
EXT_DIR="${HOME}/.local/share/gnome-shell/extensions/${EXT_UUID}"
BIN_DIR="${HOME}/.local/bin"
SERVICE_DIR="${HOME}/.config/systemd/user"
VOSK_VERSION="0.3.45"
VOSK_ARCHIVE="vosk-linux-x86_64-${VOSK_VERSION}.zip"
VOSK_URL="https://github.com/alphacep/vosk-api/releases/download/v${VOSK_VERSION}/${VOSK_ARCHIVE}"
VOSK_MODEL_NAME="vosk-model-small-en-us-0.15"
VOSK_MODEL_ARCHIVE="${VOSK_MODEL_NAME}.zip"
VOSK_MODEL_URL="https://alphacephei.com/vosk/models/${VOSK_MODEL_ARCHIVE}"

mkdir -p "$MODEL_DIR" "$VOICE_DIR" "$VOSK_DIR" "$BIN_DIR" "$SERVICE_DIR"

echo "== Quinn preflight =="

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo is required to build Quinn." >&2
    exit 1
fi
echo "cargo: ok"

if [[ "$(uname -m)" != "x86_64" ]]; then
    echo "error: Quinn's bundled Vosk library currently supports x86_64 Linux only." >&2
    exit 1
fi
echo "architecture: x86_64"

if ! command -v pkg-config >/dev/null 2>&1 || ! pkg-config --exists alsa; then
    echo "error: ALSA development files are required for voice support." >&2
    echo "Install them with: sudo apt install pkg-config libasound2-dev" >&2
    exit 1
fi
echo "ALSA development files: ok"

if ! command -v systemctl >/dev/null 2>&1; then
    echo "error: systemctl is required for the Quinn user service." >&2
    exit 1
fi
if ! systemctl --user show-environment >/dev/null 2>&1; then
    echo "error: the current user systemd session is unavailable." >&2
    exit 1
fi
echo "systemd user session: ok"

if [[ ! -f "$MODEL_DIR/needle2.cact" ]]; then
    if ! command -v hf >/dev/null 2>&1; then
        echo "error: the Hugging Face CLI 'hf' is required for the first Needle v2 model download." >&2
        echo "Install the standalone 'hf' CLI, then rerun ./install.sh." >&2
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
    echo "Vosk native library: already installed; reusing it."
fi

if [[ ! -f "$VOSK_MODEL_DIR/am/final.mdl" ]]; then
    if ! command -v curl >/dev/null 2>&1 || ! command -v unzip >/dev/null 2>&1; then
        echo "error: curl and unzip are required to install the Vosk speech model." >&2
        exit 1
    fi
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
echo "Building quinn-daemon with voice support..."
cargo build --release --manifest-path "$ROOT/Cargo.toml" -p quinn-daemon --features voice

if [[ ! -x "$ROOT/target/release/quinn-daemon" ]]; then
    echo "error: Quinn daemon build completed without producing target/release/quinn-daemon." >&2
    exit 1
fi
install -m 0755 "$ROOT/target/release/quinn-daemon" "$BIN_DIR/quinn-daemon"

if command -v gnome-extensions >/dev/null 2>&1; then
    gnome-extensions disable "$EXT_UUID" >/dev/null 2>&1 || true
fi

echo "Installing Quinn voice clips..."
cp -a "$ROOT/Voices/female/." "$VOICE_DIR/"

rm -rf "$EXT_DIR"
mkdir -p "$EXT_DIR"
cp -a "$ROOT/extension/quinn@2moresym.org/." "$EXT_DIR/"

if [[ ! -f "$EXT_DIR/metadata.json" ]]; then
    echo "error: installed GNOME extension is missing metadata.json." >&2
    exit 1
fi
if [[ ! -f "$EXT_DIR/schemas/org.gnome.shell.extensions.quinn.gschema.xml" ]]; then
    echo "error: installed GNOME extension is missing the Quinn GSettings schema." >&2
    exit 1
fi

if ! command -v glib-compile-schemas >/dev/null 2>&1; then
    echo "error: glib-compile-schemas is required for the Quinn extension settings." >&2
    exit 1
fi
glib-compile-schemas "$EXT_DIR/schemas"

cp "$ROOT/systemd/quinn.service" "$SERVICE_DIR/quinn.service"
sed -i "s|^Environment=RUST_LOG=.*$|Environment=RUST_LOG=quinn_daemon=info\\nEnvironment=QUINN_STT_MODEL=%h/.local/share/quinn/voice-model|" "$SERVICE_DIR/quinn.service"
systemctl --user daemon-reload
systemctl --user enable --now quinn.service

if ! systemctl --user is-active --quiet quinn.service; then
    echo "error: Quinn daemon service failed to start." >&2
    systemctl --user --no-pager --full status quinn.service || true
    exit 1
fi

echo "Quinn daemon: active"

if command -v gnome-extensions >/dev/null 2>&1; then
    if ! gnome-extensions enable "$EXT_UUID"; then
        echo "error: GNOME rejected the Quinn extension." >&2
        exit 1
    fi
    extension_state=$(gnome-extensions info "$EXT_UUID" 2>/dev/null | sed -n 's/^  State: //p' || true)
    if [[ "$extension_state" != "ACTIVE" ]]; then
        echo "error: Quinn extension did not reach ACTIVE state (state: ${extension_state:-unknown})." >&2
        exit 1
    fi
    echo "GNOME extension: active"
else
    echo "warning: gnome-extensions is unavailable; enable Quinn manually after installation." >&2
fi

echo
echo 'Quinn installed successfully. Test the daemon with:'
echo '  busctl --user introspect org.quinn.Assistant /org/quinn/Assistant'
echo 'Then try:'
echo '  busctl --user call org.quinn.Assistant /org/quinn/Assistant Ask s "open terminal"'
