#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXT_UUID="mx4-cursor-tracker@local"

echo "=== MX Master 4 Haptic Feedback Daemon — Installer ==="

# 1. Daemon
echo "[1/4] Installing daemon..."
install -Dm755 "$SCRIPT_DIR/mx4-haptic-daemon" "$HOME/.local/bin/mx4-haptic-daemon"

# 2. Config (don't overwrite existing)
echo "[2/4] Installing config..."
CONFIG_DIR="$HOME/.config/mx4-haptic"
if [ ! -f "$CONFIG_DIR/config.yaml" ]; then
    install -Dm644 "$SCRIPT_DIR/config.yaml" "$CONFIG_DIR/config.yaml"
else
    echo "  Config already exists, skipping (see $CONFIG_DIR/config.yaml)"
fi

# 3. GNOME Shell extension
echo "[3/4] Installing GNOME Shell extension..."
EXT_DIR="$HOME/.local/share/gnome-shell/extensions/$EXT_UUID"
install -Dm644 "$SCRIPT_DIR/gnome-extension/metadata.json" "$EXT_DIR/metadata.json"
install -Dm644 "$SCRIPT_DIR/gnome-extension/extension.js" "$EXT_DIR/extension.js"

# 4. systemd service
echo "[4/4] Installing systemd service..."
install -Dm644 "$SCRIPT_DIR/mx4-haptic.service" \
    "$HOME/.config/systemd/user/mx4-haptic.service"
systemctl --user daemon-reload
systemctl --user enable mx4-haptic.service

echo ""
echo "=== Installation complete ==="
echo ""
echo "Next steps:"
echo "  1. Log out and log back in (required to load the GNOME extension)"
echo "  2. After login, enable the extension:"
echo "       gnome-extensions enable $EXT_UUID"
echo "  3. The daemon starts automatically. Verify with:"
echo "       systemctl --user status mx4-haptic"
echo ""
echo "Quick test (no restart needed):"
echo "  mx4-haptic-daemon --test"
echo ""
echo "Configuration: $CONFIG_DIR/config.yaml"
echo "  - Adjust device_path if your mouse is not on /dev/hidraw4"
echo "  - Find your device: ls -la /dev/hidraw*"
