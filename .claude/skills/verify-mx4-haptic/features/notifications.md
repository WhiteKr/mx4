# Notifications

When any application shows a desktop notification, the mouse plays `HAPPY_ALERT`. Further notifications within the cooldown are silent.

## Sub-features

- `notify-fire` one notification sends one `HAPPY_ALERT` (0x05) packet.
- `notify-cooldown` another notification within `notifications.cooldown_s` (2 s) sends nothing.
- `notify-after-cooldown` a notification after the cooldown fires again.

## How to get to it (user POV)

- Any app calls `org.freedesktop.Notifications.Notify` (for example `notify-send "Build finished"`). The daemon eavesdrops on that call; it does not need the notification to be displayed.

## Driving it with verify.sh

Preconditions:

- Baseline from `README.md`; no notify step in the last 2 s; doctor shows `owns org.freedesktop.Notifications` ok.

- **Notify.** An app posts "Build finished". Run `$V notify "Build finished"`. Log shows `Notify: Build finished`, one packet `… wave=0x05 HAPPY_ALERT`. `notifyd.log` in the evidence dir has `Notify id=1 … summary=Build finished`.
- **Cooldown.** A second notification right away. Run `$V notify "Second" --expect 0`. No packet, but `notifyd.log` still records `summary=Second` (the call went through; the daemon chose not to buzz).
- **After cooldown.** Run `sleep 2` then `$V notify "Third"`. One `HAPPY_ALERT` packet.
- **Proof.** Run `$V evidence notifications`.

## Gotchas

- Without a process owning `org.freedesktop.Notifications`, dbus-daemon rejects `Notify` with `ServiceUnknown` before routing it and the daemon never sees it. That is why launch starts `fake-notifyd.py`; if doctor's notifications check fails, every notify step will report 0 packets.
- The daemon logs `args[3]` as the summary. A wrong argument order in a future change would show the wrong text in the log while still buzzing; compare the log summary to what you sent.
- Detection relies on an `eavesdrop=true` match rule. The private bus is a stock `dbus-daemon`, which allows it. Whether the user's real session bus does (dbus-daemon vs dbus-broker, policy) is not proven by this harness.
