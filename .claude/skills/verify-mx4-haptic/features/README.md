# mx4-haptic verification map

This directory is the maintained source for verifying what a user of mx4-haptic feels: a haptic pulse on the MX Master 4 when something happens on the GNOME desktop. Read this index before driving, then use the matching feature file as the recipe. The harness is `scripts/verify.sh` (written `$V` below, see `../SKILL.md`).

## Baseline preconditions

- `V=.claude/skills/verify-mx4-haptic/scripts/verify.sh`, run from the repo root.
- `$V launch` succeeded and `$V doctor` shows no `FAIL` line.
- The run uses the repo's `config.yaml` with only `device_path` replaced by the fake device. Its values decide expectations: boundary debounce 100 ms, notification cooldown 2 s, snap debounce 300 ms, focus cooldown 0.3 s, workspace has none. Every trigger is enabled. Waveforms: `HAPPY_ALERT` (0x05) for notifications, `SUBTLE_COLLISION` (0x04) for everything else.
- Never drive a daemon this run did not start, and never a real `/dev/hidraw*`.

## Driving conventions

- One trigger command = one user action. Its output (log lines + decoded packets + `ok:` line) is the per-step proof; a non-zero exit is a failed step.
- Leave at least 0.5 s between unrelated steps of the same feature, so an earlier step's debounce or cooldown window does not swallow the next one. The helper already waits 0.5 s after each trigger, which covers every window except the 2 s notification cooldown.
- The daemon reads config only at start. To test a variation (a disabled trigger, another waveform, a longer debounce), copy the repo `config.yaml`, edit the copy, and launch with `MX4_CONFIG=/path/to/copy.yaml $V launch`. Keep a top-level `device_path:` line; launch replaces it with the fake device. The edited config is saved in the evidence dir at cleanup.
- Finish a journey with `$V evidence <feature-id>`, then `$V cleanup`.

## Proof and skip reporting

- A step is proven by the decoded packet (right waveform byte, not `MALFORMED`) together with the matching daemon log line. Neither alone is enough.
- Suppression (debounce, cooldown) is proven by `--expect 0` or `--repeat N --expect 1` passing.
- Record the run id and git revision from `$V doctor` with every report.
- The extension itself (gnome-shell), the real session bus, and the real mouse are outside this harness. Report them as not verified unless a human-in-the-loop check from `../SKILL.md` "Real hardware" was done.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing what the user experiences. It then has exactly four H2 sections in this order: `Sub-features`, `How to get to it (user POV)`, `Driving it with verify.sh` (starting with `Preconditions:`), and `Gotchas`.

## Features

- [Display boundary](./display-boundary.md): pulse when the cursor moves to another monitor, with debounce.
- [Notifications](./notifications.md): pulse when any app posts a desktop notification, with cooldown.
- [Workspace switch](./workspace-switch.md): pulse on every workspace change.
- [Window snap](./window-snap.md): pulse when a window is tiled left/right or maximized, with debounce.
- [Focus change](./focus-change.md): pulse when another window gets focus, with cooldown.
- [Waveform test](./waveform-test.md): `mx4-haptic-daemon --test` plays all 16 waveforms.
