# Workspace switch

Each time the user switches GNOME workspace, the mouse gives a light bump. There is no debounce: every switch buzzes.

## Sub-features

- `ws-fire` a switch sends one `SUBTLE_COLLISION` (0x04) packet.
- `ws-no-debounce` N quick switches send N packets.

## How to get to it (user POV)

- Switch workspace with `Super+Page Down`/`Super+Page Up`, a touchpad gesture, or the overview. The extension emits `WorkspaceSwitched(from, to)` on `active-workspace-changed`.

## Driving it with verify.sh

Preconditions:

- Baseline from `README.md`.

- **Switch.** Go from workspace 0 to 1. Run `$V workspace 0 1`. Log `WorkspaceSwitched: 0, 1`, one `SUBTLE_COLLISION` packet.
- **Rapid switches.** Run `$V workspace 1 2 --repeat 3 --expect 3`. Three packets.
- **Proof.** Run `$V evidence workspace-switch`.

## Gotchas

- `workspace_switch` has no debounce key in the shipped config. Adding `debounce_ms` or `cooldown_s` to it enables throttling like any other trigger; then update this file and the `--expect` count.
- The extension only emits when the index actually changes; the harness can emit `0 0` and the daemon will still buzz. That is a harness artifact, not a daemon bug.
