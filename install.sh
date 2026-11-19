#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXT_UUID="mx4-cursor-tracker@local"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/mx4"

echo "=== mx4 (MX Master 4 daemon) — Installer ==="

command -v cargo >/dev/null || {
    echo "cargo not found. Install Rust first: https://rustup.rs" >&2
    exit 1
}

echo "[1/5] Building mx4d..."
cargo build --release --locked --manifest-path "$SCRIPT_DIR/Cargo.toml"
install -Dm755 "$SCRIPT_DIR/target/release/mx4d" "$HOME/.local/bin/mx4d"

echo "[2/5] Installing config..."
if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    install -Dm644 "$SCRIPT_DIR/config.toml" "$CONFIG_DIR/config.toml"
else
    echo "  Config already exists, skipping (see $CONFIG_DIR/config.toml)"
fi

echo "[3/5] Installing GNOME Shell extension..."
EXT_DIR="$HOME/.local/share/gnome-shell/extensions/$EXT_UUID"
install -Dm644 "$SCRIPT_DIR/gnome-extension/metadata.json" "$EXT_DIR/metadata.json"
install -Dm644 "$SCRIPT_DIR/gnome-extension/extension.js" "$EXT_DIR/extension.js"

echo "[4/5] Removing the old Python daemon, if installed..."
if [ -f "$HOME/.config/systemd/user/mx4-haptic.service" ]; then
    systemctl --user disable --now mx4-haptic.service || true
    rm -f "$HOME/.config/systemd/user/mx4-haptic.service"
fi
rm -f "$HOME/.local/bin/mx4-haptic-daemon"
if [ -f "$HOME/.config/mx4-haptic/config.yaml" ]; then
    echo "  Old config left at ~/.config/mx4-haptic/config.yaml; mx4d reads $CONFIG_DIR/config.toml"
fi

echo "[5/5] Installing systemd service..."
install -Dm644 "$SCRIPT_DIR/mx4d.service" "$HOME/.config/systemd/user/mx4d.service"
systemctl --user daemon-reload
systemctl --user enable mx4d.service
systemctl --user restart mx4d.service || true

echo ""
echo "=== Installation complete ==="
echo ""
echo "Next steps:"
echo "  1. If the extension is new, log out and back in, then:"
echo "       gnome-extensions enable $EXT_UUID"
echo "  2. Check the daemon:"
echo "       systemctl --user status mx4d"
echo ""
echo "Quick test:  mx4d --test"
echo "Config:      $CONFIG_DIR/config.toml"
