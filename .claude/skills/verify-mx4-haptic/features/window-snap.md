# Window snap

When the user tiles a window to the left or right half, or maximizes it, the mouse gives a light bump. Snap animations that resize several times produce one bump.

## Sub-features

- `snap-fire` `left`, `right` or `maximize` sends one `SUBTLE_COLLISION` (0x04) packet.
- `snap-debounce` a second snap within `window_snap.debounce_ms` (300 ms) sends nothing.

## How to get to it (user POV)

- Drag a window to a screen edge, use `Super+Left`/`Super+Right`, or double-click the title bar / `Super+Up` to maximize. The extension watches the focused window's size and emits `WindowSnapped(type, monitor)` when it settles in a new snap state.

## Driving it with verify.sh

Preconditions:

- Baseline from `README.md`; no snap step in the last 300 ms.

- **Snap left.** Run `$V snap left 0`. Log `WindowSnapped: left, 0`, one packet.
- **Debounce.** Run `$V snap right 0 --repeat 2 --expect 1`. One packet.
- **Maximize.** After the helper's 0.5 s pause, run `$V snap maximize 1`. Log `WindowSnapped: maximize, 1`, one packet.
- **Proof.** Run `$V evidence window-snap`.

## Gotchas

- Snap detection only tracks the focused window and needs a focus change first to start tracking. The harness skips that, so it cannot catch a regression in the extension's tracking logic.
- The extension also debounces (100 ms) before emitting; the daemon's 300 ms debounce is a second layer. This harness exercises only the daemon's.
- The daemon logs any string as the snap type; it does not validate `left|right|maximize`.
