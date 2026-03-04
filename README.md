# mx4-haptic

Haptic feedback daemon for the **Logitech MX Master 4** on Linux (GNOME/Wayland).

On Windows/macOS, Logi Options+ provides haptic feedback for display transitions and notifications. This project brings that experience to Linux by sending HID++2.0 commands directly to the mouse via `/dev/hidraw`, achieving sub-millisecond response times.

## Features

- **Display boundary haptic** — vibration when the cursor moves between monitors
- **Notification haptic** — vibration when a desktop notification arrives
- 16 built-in waveforms (configurable per trigger)
- Near-instant response via direct HID++ writes (no Solaar overhead)
- Runs as a systemd user service

## Requirements

- Logitech MX Master 4 (connected via Bluetooth)
- GNOME 46 on Wayland (Ubuntu 24.04+)
- Python 3.10+
- Python packages: `dbus-python`, `PyGObject`, `PyYAML`
- Access to `/dev/hidraw*` for your mouse (see [Device Access](#device-access))

## Installation

```bash
git clone https://github.com/whitekr/mx4-haptic.git
cd mx4-haptic
./install.sh
```

Then **log out and log back in** to load the GNOME Shell extension, and run:

```bash
gnome-extensions enable mx4-cursor-tracker@local
```

### Quick Test

```bash
mx4-haptic-daemon --test
```

This plays all 16 waveforms sequentially — you should feel each one on the mouse.

## Configuration

Edit `~/.config/mx4-haptic/config.yaml`:

```yaml
device_path: /dev/hidraw4        # adjust to your device
feature_index: 11                # MX Master 4 HAPTIC feature index
device_number: 0xFF              # 0xFF for Bluetooth direct

display_boundary:
  enabled: true
  waveform: SUBTLE_COLLISION     # waveform on monitor crossing
  poll_interval_ms: 50
  debounce_ms: 100               # min interval between triggers

notifications:
  enabled: true
  waveform: HAPPY_ALERT          # waveform on notification
  cooldown_s: 2                  # min interval between triggers
```

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
sudo tee /etc/udev/rules.d/99-mx4-haptic.rules << 'EOF'
# Logitech MX Master 4 — allow user access for haptic feedback
KERNEL=="hidraw*", ATTRS{idVendor}=="046d", MODE="0660", TAG+="uaccess"
EOF

sudo udevadm control --reload-rules
sudo udevadm trigger
```

Alternatively, set an ACL on the specific device:

```bash
sudo setfacl -m u:$USER:rw /dev/hidraw4
```

## Architecture

```
┌─────────────────────────────────┐
│  GNOME Shell Extension          │
│  (mx4-cursor-tracker@local)     │
│                                 │
│  Polls global.get_pointer()     │
│  every 50ms inside compositor   │
│         │                       │
│         │ D-Bus signal:         │
│         │ BoundaryCrossed       │
└─────────┼───────────────────────┘
          ▼
┌─────────────────────────────────┐
│  mx4-haptic-daemon              │
│                                 │
│  ┌──────────┐  ┌─────────────┐  │
│  │ Boundary  │  │ Notification│  │
│  │ Detector  │  │ Listener    │  │
│  │ (D-Bus)   │  │ (D-Bus)    │  │
│  └─────┬─────┘  └──────┬─────┘  │
│        └────────┬───────┘        │
│                 ▼                │
│          HapticSender            │
│          /dev/hidraw ──► Mouse   │
└─────────────────────────────────┘
```

### Why not Solaar?

Solaar's `solaar config <device> haptic-play <waveform>` works but takes ~1.5 seconds due to D-Bus IPC, Python object serialization, feature table negotiation, and response waiting. This daemon writes the 20-byte HID++ packet directly to `/dev/hidraw`, completing in under 1ms.

### Why a GNOME extension?

On GNOME 46 Wayland, there is no userspace API for cursor position tracking. XWayland's `query_pointer` only updates when the cursor is over an X11 window. The GNOME Shell extension runs inside the Mutter compositor and can call `global.get_pointer()` directly.

## Service Management

```bash
# Status
systemctl --user status mx4-haptic

# Logs
journalctl --user -u mx4-haptic -f

# Restart after config change
systemctl --user restart mx4-haptic

# Disable
systemctl --user disable --now mx4-haptic
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

## Dependencies

```bash
# Ubuntu/Debian
sudo apt install python3 python3-dbus python3-gi python3-yaml
```

## License

MIT
