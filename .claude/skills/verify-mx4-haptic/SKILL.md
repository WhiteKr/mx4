---
name: verify-mx4-haptic
description: Launch and drive mx4d (the MX Master 4 daemon for GNOME/Linux) with its haptic module in an isolated sandbox and prove which HID++ haptic packets it sends for each trigger (monitor boundary, notification, workspace switch, window snap, focus change, --test). Use when you change the daemon, its config handling, or the D-Bus contract with the GNOME extension and need evidence it still behaves, without a mouse or a GNOME session.
---

# Verify mx4-haptic

The product is a Rust daemon (`mx4d`, built from `crates/`; the triggers live in `crates/mx4-haptic`) plus a GNOME Shell extension (`gnome-extension/`). The user "touches" it through the mouse: an action on the desktop makes the mouse vibrate. Under the hood that is:

```
GNOME extension ──D-Bus signal──▶ daemon ──20-byte HID++ write──▶ /dev/hidrawN (mouse)
any app ──Notify() call──▶ notification server   (daemon eavesdrops this call)
```

An agent has no mouse and usually no GNOME session, so this skill swaps both ends for stand-ins and keeps the daemon itself real, unmodified, started from this checkout:

- **Signal source:** a private `dbus-daemon --session`. `gdbus emit` sends the exact signals the extension sends (same interface, path, argument types as `gnome-extension/extension.js`).
- **Mouse:** a regular file. The daemon opens it as `[device] path` and every haptic becomes 20 bytes appended to it, which the helper decodes.
- **Notification server:** `scripts/fake-notifyd.py` owns `org.freedesktop.Notifications` on the private bus (verification scaffolding; gnome-shell owns it in production).

Everything goes through one helper: `.claude/skills/verify-mx4-haptic/scripts/verify.sh`. Run it with no arguments for usage. Below, `V` means that path:

```bash
V=.claude/skills/verify-mx4-haptic/scripts/verify.sh   # from the repo root
```

The feature recipes are in [`features/README.md`](features/README.md). Read it before driving.

## Isolation and safety

- Each `launch` gets its own bus, fake device, config and run dir under `/tmp/mx4-verify/<RUN_ID>/`. Parallel runs are safe; pick one with `MX4_RUN=<RUN_ID>` (default: the latest launch).
- The run never touches the user's real session bus, their installed `~/.local/bin/mx4d`, the `mx4d` systemd user service, `~/.config/mx4/`, or any `/dev/hidraw*`.
- **Never point an automated run at a real `/dev/hidraw*`.** The daemon only writes to a node sysfs identifies as the MX Master 4 (or its receiver), but a real mouse would still vibrate and a misidentified node would still get packets. That is also why cleanup stops the daemon before deleting its fake device, and why doctor fails if any real hidraw is open.
- Driving the real mouse is a human-in-the-loop check. See "Real hardware" below.

## Launch

Requirements: `cargo`, `dbus-daemon`, `gdbus` (libglib2.0-bin) and a Python with `dbus-python` and `PyGObject` for the fake notification server and the packet decoder. On Ubuntu that is `apt install dbus python3-dbus python3-gi`. `launch` and `test-mode` run `cargo build --release` on this checkout first (or use `MX4_BIN=/path/to/mx4d`). The helper picks the first of `python3`, `/usr/bin/python3`, `/usr/bin/python3.12` that imports both; override with `MX4_PY=/path/to/python`. (In the Claude cloud container `python3` is a 3.13 without the dbus bindings and the helper falls back to `/usr/bin/python3.12`.)

```bash
$V launch
# RUN_ID=run-20261119-082704-773
# evidence: /root/mx4-verify-evidence/run-20261119-082704-773
```

`launch` copies the repo's `config.toml` (or `MX4_CONFIG=<file>` if set) with only the `path = ` line under `[device]` rewritten to the fake device, starts the bus, the fake notification server and the daemon (`--verbose`), and returns once the daemon log has `mx4d started (device=…)` and the notification server printed `fake-notifyd ready`. Both must be true: the start line alone does not mean notifications can be delivered. If it exits non-zero, read the printed log and run `$V cleanup`.

There is no data dir or migration; an empty run dir is the starting state. Teardown is `$V cleanup`.

## Doctor

Read-only. Run it right after launch and whenever anything looks off:

```bash
$V doctor
```

It checks: bus, daemon and notification server pids are alive and are the processes this run started (matched by command line, not by name); the daemon holds the fake device open and holds no real `/dev/hidraw*`; the bus answers and `org.freedesktop.Notifications` has an owner; the daemon logged its start line. It prints the run id, the git revision under test (with `-dirty` if the checkout has uncommitted changes) and the packet count so far. Any `FAIL` line means do not drive: clean up and relaunch.

## Drive

Each trigger command fires the real D-Bus event, waits up to 3 s, then prints the new daemon log lines and the decoded new packets, and exits non-zero unless exactly the expected number of packets appeared (and, when one is expected, the matching log line).

```bash
$V boundary FROM TO X Y      # BoundaryCrossed(iiii)       log: "BoundaryCrossed: A, B, X, Y"
$V workspace FROM TO         # WorkspaceSwitched(ii)       log: "WorkspaceSwitched: A, B"
$V snap TYPE MONITOR         # WindowSnapped(si), TYPE left|right|maximize  log: "WindowSnapped: TYPE, MONITOR"
$V focus NAME                # FocusChanged(s)             log: "FocusChanged: NAME"
$V notify SUMMARY            # Notify() to org.freedesktop.Notifications   log: "Notify: SUMMARY"
```

Options for every trigger: `--repeat N` fires N events back to back (about 20 ms apart), `--expect K` asserts exactly K new packets (default 1, `0` asserts suppression). Use them for cooldowns:

```bash
$V boundary 0 1 1920 540 --repeat 2 --expect 1   # 100 ms cooldown swallows the second
$V notify "Second" --expect 0                     # within 2 s cooldown of the previous notify
```

`$V test-mode` runs `mx4d --test` against its own fresh fake device (no bus needed, about 24 s) and decodes all 16 waveforms.

A decoded packet looks like:

```
#001 11 ff 0b 41 04 00 … 00  dev=0xFF feat=11 wave=0x04 SUBTLE_COLLISION
```

Byte 0 `0x11` is the HID++ long report, byte 1 the device number from config (`0xFF` = Bluetooth direct), byte 2 the haptic feature index (`11`), byte 3 `0x41` the function/software id, byte 4 the waveform, bytes 5-19 zero. Any deviation is flagged `MALFORMED`.

## Evidence

Evidence lives outside the scratch dir and survives cleanup: `${MX4_VERIFY_EVIDENCE:-$HOME/mx4-verify-evidence}/<RUN_ID>/`.

| File | Written by | Proves |
| --- | --- | --- |
| `daemon.log` | launch (live) | which events the daemon received and what it decided |
| `notifyd.log` | fake-notifyd (live) | the Notify call really reached a server and got an id |
| `packets-<label>.txt`, `fake-hidraw-<label>.bin` | `$V evidence <label>` | the exact bytes that would have gone to the mouse, decoded and raw |
| `packets-final.txt`, `fake-hidraw-final.bin`, `config.toml` | cleanup | full packet history and the config actually used |
| `INDEX` | `$V evidence` | run id, git revision, label, timestamp per snapshot |
| `test-mode.out`, `packets-test-mode.txt` | `$V test-mode` (dir `test-<ts>`) | `--test` output, exit code and all 16 packets |

Proof standards:

- Drive through the real interface: emit the D-Bus signal or make the Notify call. Never import the daemon and call `HapticSender.play()` or a detector's `_on_*` method; that skips the D-Bus wiring, which is the thing most likely to break.
- Capture the action and the result: the trigger output (log line + decoded packet) for each step, then `$V evidence <label>` at the end of the journey.
- The side effect is the packet bytes, not the log line. A log line with no packet (or a packet with the wrong waveform byte) is a failure.
- For "should not fire" behavior (cooldown, `enabled = false`), assert `--expect 0`, not just the absence of a log line.
- The stand-ins sit at real boundaries (D-Bus and the device file), so the daemon code under test is unchanged. What this harness does not prove: that the extension emits these signals inside gnome-shell, that the mouse accepts the packet, and that the user's real session bus delivers eavesdropped Notify calls. Say so in any report.

## Cleanup

```bash
$V cleanup                       # latest run
MX4_RUN=<RUN_ID> $V cleanup      # a specific run
```

It snapshots the final packets and config into the evidence dir, stops the daemon first, then the notification server, then the bus (each only if its pid still matches the command line this run launched it with; never by process name), and removes `/tmp/mx4-verify/<RUN_ID>/`. It prints the evidence path and its contents; check they are there. Run cleanup after every failed iteration too. `test-mode` cleans its own scratch dir.

## Real hardware (human in the loop, unverified by this skill)

Only with the user present, on their Ubuntu/GNOME 46 machine with the mouse connected, and only when they ask. The installed service and the real device are theirs; do not start a second daemon against the same device.

- Doctor: `systemctl --user is-active mx4d`, `journalctl --user -u mx4d -n 30`, `gnome-extensions info mx4-cursor-tracker@local` (State: ACTIVE), and `gdbus call --session --dest org.gnome.Shell --object-path /com/mx4haptic/CursorTracker --method com.mx4haptic.CursorTracker.GetPosition` (returns `(x, y, monitor)`). Find the mouse with the loop in README "Finding Your Device".
- Watch the extension's signals while the user moves the cursor across monitors, switches workspace, snaps a window, or changes focus: `dbus-monitor --session "type='signal',interface='com.mx4haptic.CursorTracker'"`, alongside `journalctl --user -u mx4d -f`.
- Waveforms on the real mouse: `mx4d --test` (the user confirms they feel 16 pulses; the output must show 16 `OK`).

## Helpers

- `scripts/verify.sh`: everything above. Executable; `$V` with no args prints usage. Env: `MX4_RUN`, `MX4_CONFIG`, `MX4_PY`, `MX4_VERIFY_SCRATCH` (default `/tmp/mx4-verify`), `MX4_VERIFY_EVIDENCE` (default `$HOME/mx4-verify-evidence`).
- `scripts/fake-notifyd.py`: started by `launch`; not meant to be run by hand.
