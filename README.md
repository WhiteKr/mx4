# mx4

A daemon for the **Logitech MX Master 4** on Linux (GNOME/Wayland), built one feature at a time.

`mx4d` owns the connection to the mouse (HID++ 2.0 over `/dev/hidraw`) and runs the
feature modules that are enabled in its config. Today there is one module:

- **haptic** — vibration on desktop events, which Logi Options+ offers only on Windows/macOS:
  - **Display boundary** — the cursor moves between monitors
  - **Workspace switch** — the active workspace changes
  - **Window snap** — a window is maximized or tiled left/right
  - **Focus change** — a different window takes focus
  - **Notification** — a desktop notification arrives
  - 16 built-in waveforms, configurable per trigger, sent in under 1 ms

The daemon finds the mouse automatically and reconnects after sleep or re-pairing.

## Layout

```
crates/mx4-core     mouse connection: hidraw discovery, HID++ requests, haptics
crates/mx4-haptic   haptic module: desktop-event triggers
crates/mx4d         the daemon binary; each module is a cargo feature
gnome-extension/    GNOME Shell extension that reports desktop events over D-Bus
```

A new feature goes in its own `crates/mx4-<name>` crate that depends only on
`mx4-core`, plus a cargo feature and a config section in `mx4d`.

## Requirements

- Logitech MX Master 4 (Bluetooth, or a Bolt receiver)
- GNOME Shell 46–50 on Wayland (Ubuntu 24.04 or newer)
- Rust toolchain to build (`cargo`, from [rustup.rs](https://rustup.rs))
- Access to `/dev/hidraw*` for your mouse (see [Device Access](#device-access))

## Installation

```bash
git clone https://github.com/WhiteKr/mx4.git
cd mx4
./install.sh
```

`install.sh` builds `mx4d`, installs it to `~/.local/bin`, installs the GNOME extension
and the `mx4d` systemd user service, and removes the old Python `mx4-haptic` service if
it finds one. If the extension is new, log out and back in, then run:

```bash
gnome-extensions enable mx4-cursor-tracker@local
```

### Quick Test

```bash
mx4d --test
```

This plays all 16 waveforms in a row; you should feel each one on the mouse.

## Configuration

Edit `~/.config/mx4/config.toml` and restart the service. Keys you leave out keep their
defaults, so a config can be as short as one changed waveform. See
[`config.toml`](config.toml) for every key:

```toml
[device]
path = "auto"          # or a fixed node such as "/dev/hidraw4"
number = 0xFF          # 0xFF for Bluetooth direct, 1-6 for a Bolt receiver slot
# feature_index = 11   # HAPTIC feature index; asked from the mouse when unset

[haptic.notifications]
waveform = "HAPPY_ALERT"
cooldown_ms = 2000     # events closer together play once

[haptic.focus_change]
enabled = false
```

hidraw numbers can change after a reboot or reconnect, so the daemon checks `path`
against sysfs before every open. It only writes to a Logitech (`046d`) HID++ device whose
name contains "MX Master 4", or, when `number` is a receiver slot (1–6), to a Logitech
receiver. Otherwise it scans `/sys/class/hidraw` for a matching device and never falls
back to an unrelated one. Once open, it asks the mouse where its HAPTIC feature lives
instead of assuming an index.

### Available Waveforms

| Name | Index | Description |
|------|-------|-------------|
| SHARP_STATE_CHANGE | 0x00 | Sharp click |
| DAMP_STATE_CHANGE | 0x01 | Dampened click |
| SHARP_COLLISION | 0x02 | Sharp bump |
| DAMP_COLLISION | 0x03 | Dampened bump |
| SUBTLE_COLLISION | 0x04 | Light bump |
| HAPPY_ALERT | 0x05 | Pleasant notification |
| ANGRY_ALERT | 0x06 | Urgent notification |
| COMPLETED | 0x07 | Task complete |
| SQUARE | 0x08 | Square wave |
| WAVE | 0x09 | Sine wave |
| FIREWORK | 0x0A | Burst pattern |
| MAD | 0x0B | Intense vibration |
| KNOCK | 0x0C | Knock pattern |
| JINGLE | 0x0D | Musical pattern |
| RINGING | 0x0E | Ring pattern |
| WHISPER_COLLISION | 0x1B | Very subtle bump |

## Device Access

The daemon needs read/write access to `/dev/hidraw*`. The recommended approach is a udev rule:

```bash
# Find your device
ls -la /dev/hidraw*

# Create a udev rule (adjust idVendor/idProduct if needed)
sudo tee /etc/udev/rules.d/99-mx4.rules << 'EOF'
# Logitech MX Master 4 — allow user access for haptic feedback
KERNEL=="hidraw*", ATTRS{idVendor}=="046d", MODE="0660", TAG+="uaccess"
EOF

sudo udevadm control --reload-rules
sudo udevadm trigger
```

Alternatively, set an ACL on the specific device:

```bash
sudo setfacl -m u:$USER:rw /dev/hidrawN   # the node from "Finding Your Device"
```

## Architecture

```
GNOME Shell extension ──D-Bus signals──┐
  (BoundaryCrossed, WorkspaceSwitched,  │
   WindowSnapped, FocusChanged)         ▼
                                 ┌──────────────────────────┐
any app ──Notify()──▶ notifier   │ mx4d                     │
          (watched as a monitor)─▶  haptic module (triggers)│
                                 │        │                 │
                                 │  mx4-core (Haptics,      │
                                 │  Device) ──/dev/hidraw──▶ Mouse
                                 └──────────────────────────┘
```

### Why not Solaar?

Solaar's `solaar config <device> haptic-play <waveform>` takes about 1.5 seconds. `mx4d`
writes the 20-byte HID++ packet straight to `/dev/hidraw` in under 1 ms.

### Why a GNOME extension?

On GNOME Wayland, there is no userspace API for cursor position tracking. XWayland's `query_pointer` only updates when the cursor is over an X11 window. The GNOME Shell extension runs inside the Mutter compositor and can call `global.get_pointer()` directly.

## Service Management

```bash
systemctl --user status mx4d          # status
journalctl --user -u mx4d -f          # logs
systemctl --user restart mx4d         # after a config change
systemctl --user disable --now mx4d   # turn off
gnome-extensions disable mx4-cursor-tracker@local
```

## Finding Your Device

```bash
# List hidraw devices
ls -la /dev/hidraw*

# Check which device is your MX Master 4
for d in /dev/hidraw*; do
    echo -n "$d: "
    cat "/sys/class/hidraw/$(basename $d)/device/uevent" 2>/dev/null | grep HID_NAME || echo "unknown"
done

# Or use Solaar
solaar show
```

You only need this when the daemon logs that it cannot find the mouse.

## Development

```bash
cargo test                                         # unit tests, no mouse or GNOME needed
cargo build --release && sudo tests/gnome-smoke/run.sh   # headless GNOME Shell end-to-end test (throwaway machine)
```

CI runs both, plus `cargo fmt --check` and `cargo clippy`, on every pull request.

## License

MIT
