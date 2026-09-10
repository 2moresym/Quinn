#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
MODEL_DIR="${HOME}/.local/share/quinn/weights"
VOICE_DIR="${HOME}/.local/share/quinn/voices/female"
EXT_DIR="${HOME}/.local/share/gnome-shell/extensions/quinn@2moresym.org"
BIN_DIR="${HOME}/.local/bin"
SERVICE_DIR="${HOME}/.config/systemd/user"

mkdir -p "$MODEL_DIR" "$VOICE_DIR" "$BIN_DIR" "$SERVICE_DIR"

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo is required to build Quinn." >&2
    exit 1
fi

if ! command -v hf >/dev/null 2>&1; then
    cat >&2 <<'EOF'
error: the Hugging Face CLI 'hf' is required for the one-time model download.
Install it with: python3 -m pip install -U huggingface_hub
EOF
    exit 1
fi

if [[ ! -f "$MODEL_DIR/needle2.cact" ]]; then
    echo "Downloading Needle v2 model..."
    hf download Cactus-Compute/needle2 needle2.cact --local-dir "$MODEL_DIR"
fi

echo "Building quinn-daemon..."
cargo build --release --manifest-path "$ROOT/Cargo.toml" -p quinn-daemon
install -m 0755 "$ROOT/target/release/quinn-daemon" "$BIN_DIR/quinn-daemon"

if [[ -d "$ROOT/Voices/female" ]]; then
    echo "Installing Quinn voice clips..."
    cp -a "$ROOT/Voices/female/." "$VOICE_DIR/"
fi

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
