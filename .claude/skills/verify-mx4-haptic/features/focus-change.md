# Focus change

When a different window takes focus, the mouse gives a light bump. Focus flapping within the cooldown buzzes once.

## Sub-features

- `focus-fire` a focus change sends one `SUBTLE_COLLISION` (0x04) packet.
- `focus-cooldown` another change within `haptic.focus_change.cooldown_ms` (300 ms) sends nothing.

## How to get to it (user POV)

- Click another window, `Alt+Tab`, or open a new app. The extension emits `FocusChanged(name)` with the window's WM class (or title).

## Driving it with verify.sh

Preconditions:

- Baseline from `README.md`; no focus step in the last 0.3 s.

- **Focus another app.** Run `$V focus org.gnome.Nautilus`. Log `FocusChanged: org.gnome.Nautilus`, one packet.
- **Cooldown.** `Alt+Tab` twice quickly. Run `$V focus firefox --repeat 2 --expect 1`. One packet.
- **Proof.** Run `$V evidence focus-change`.

## Gotchas

- With every trigger enabled, a real snap or workspace switch often also changes focus, so the user can feel two bumps for one action. The harness emits each signal alone and does not reproduce that overlap.
- Names with spaces or quotes: the helper wraps the name in single quotes for `gdbus emit`; a name containing `'` breaks the GVariant text. Use plain names.
