# Display boundary

When the user moves the cursor from one monitor to another, the mouse gives a light bump (`SUBTLE_COLLISION`). Rapid back-and-forth crossings inside the debounce window produce only one bump.

## Sub-features

- `boundary-fire` one crossing sends one `SUBTLE_COLLISION` (0x04) packet.
- `boundary-debounce` a second crossing within `display_boundary.debounce_ms` (100 ms in repo config) sends nothing.
- `boundary-disabled` with `display_boundary.enabled: false` crossings send nothing.

## How to get to it (user POV)

- Move the cursor across the edge between two monitors. The GNOME extension polls the pointer every 50 ms and emits `BoundaryCrossed(from, to, x, y)`.

## Driving it with verify.sh

Preconditions:

- Baseline from `README.md`; no boundary event in the last 100 ms.

- **Cross once.** Cursor goes from monitor 0 to 1. Run `$V boundary 0 1 1920 540`. Log shows `Boundary crossed: monitor 0 → 1 (x=1920)`, one packet `… wave=0x04 SUBTLE_COLLISION`, and `ok: 1 event(s) -> 1 packet(s)`.
- **Debounce.** Cursor flicks back and forth across the edge. Run `$V boundary 1 0 1900 540 --repeat 2 --expect 1`. Exactly one new packet.
- **Disabled.** Clean up, then launch with a copy of `config.yaml` that sets `display_boundary.enabled: false` (`MX4_CONFIG=… $V launch`). Run `$V boundary 0 1 1920 540 --expect 0`. Daemon log has no `Display boundary detector listening` line at start.
- **Proof.** Run `$V evidence display-boundary`.

## Gotchas

- `debounce_ms` defaults to 500 in the daemon's built-in defaults but the repo `config.yaml` sets 100, and README documents 100. Expectations follow the config file the run used (`config.yaml` in the evidence dir).
- `--repeat 2` emits about 20 ms apart; on a very slow machine two `gdbus` spawns could exceed 100 ms and the step fails with 2 packets. Check the timestamps in the log before calling it a regression.
- `poll_interval_ms` in config is not read by the daemon; polling is hard-coded to 50 ms in the extension. Changing it proves nothing here.
- The harness cannot check that the extension detects crossings; that needs the real-desktop check in `../SKILL.md`.
