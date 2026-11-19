#!/usr/bin/env bash
# End-to-end check: headless GNOME Shell + the extension + the daemon writing to a fake hidraw node.
# Runs as root in a throwaway container or CI runner (it starts a system bus and hides
# /run/systemd/seats so the shell does not wait for logind). Usage: tests/gnome-smoke/run.sh
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)

if [ "${1:-}" != "--inside" ]; then
    mkdir -p /run/dbus
    pgrep -f 'dbus-daemon --system' >/dev/null || dbus-daemon --system --fork
    if [ -d /run/systemd/seats ]; then
        mv /run/systemd/seats /run/systemd/seats.off
        trap 'mv /run/systemd/seats.off /run/systemd/seats' EXIT
    fi
    HOME=$(mktemp -d) timeout 120 dbus-run-session -- "$0" --inside
    exit
fi

W=$HOME
export XDG_RUNTIME_DIR=$W/run
mkdir -m 700 "$XDG_RUNTIME_DIR"
EXT=$W/.local/share/gnome-shell/extensions
mkdir -p "$EXT/mx4-cursor-tracker@local"
cp "$REPO"/gnome-extension/* "$EXT/mx4-cursor-tracker@local/"
cp -r "$HERE/driver@local" "$EXT/"
gsettings set org.gnome.shell disable-user-extensions false
gsettings set org.gnome.shell enabled-extensions "['mx4-cursor-tracker@local','driver@local']"
gsettings set org.gnome.mutter dynamic-workspaces false
gsettings set org.gnome.desktop.wm.preferences num-workspaces 2

gnome-shell --headless --wayland --no-x11 --virtual-monitor 1280x800 --virtual-monitor 1280x800 \
    >"$W/shell.log" 2>&1 &
for _ in $(seq 60); do
    gdbus introspect --session --dest org.gnome.Shell --object-path /smoke/Driver >/dev/null 2>&1 && break
    sleep 0.5
done

DEV=$W/fake-hidraw
: >"$DEV"
printf 'device_path: %s\n' "$DEV" >"$W/config.yaml"
python3 "$REPO/mx4-haptic-daemon" --config "$W/config.yaml" >"$W/daemon.log" 2>&1 &
python3 "$HERE/notifyd.py" &
sleep 2

drive() { gdbus call --session --dest org.gnome.Shell --object-path /smoke/Driver --method "smoke.Driver.$1" "${@:2}" >/dev/null; }
drive Warp 100 100; sleep 0.5
drive Warp 1900 300; sleep 0.5
drive NextWorkspace; sleep 0.5
GTK_A11Y=none GSK_RENDERER=cairo WAYLAND_DISPLAY=wayland-0 python3 -c "
import gi; gi.require_version('Gtk', '4.0'); from gi.repository import Gtk, GLib
app = Gtk.Application(application_id='smoke.Win')
app.connect('activate', lambda a: Gtk.ApplicationWindow(application=a, default_width=300, default_height=200).present())
GLib.timeout_add_seconds(4, app.quit); app.run([])" >/dev/null 2>&1 &
sleep 1.5
drive Maximize; sleep 1
gdbus call --session --dest org.freedesktop.Notifications --object-path /org/freedesktop/Notifications \
    --method org.freedesktop.Notifications.Notify smoke 0 "" hello body '[]' '{}' 1000 >/dev/null
sleep 1

cat "$W/daemon.log"
fail=0
for t in BoundaryCrossed WorkspaceSwitched FocusChanged WindowSnapped Notify; do
    grep -q "INFO $t:" "$W/daemon.log" || { echo "FAIL: no $t"; fail=1; }
done
packets=$(( $(stat -c %s "$DEV") / 20 ))
[ "$packets" -eq 5 ] || { echo "FAIL: $packets packets, expected 5"; fail=1; }
grep -E "JS ERROR|mx4-cursor-tracker" "$W/shell.log" || true
[ $fail -eq 0 ] && echo "PASS: 5 triggers, 5 packets"
exit $fail
