"""Verification scaffolding: a minimal org.freedesktop.Notifications server.

On a real GNOME session gnome-shell owns this name. On the private bus that
verify.sh starts nobody does, and dbus-daemon answers Notify with
ServiceUnknown without routing it, so mx4-haptic-daemon never sees the call.
This stand-in owns the name and replies like a real server so the daemon can
observe the Notify call the way it does in production.

Run only through verify.sh; it expects DBUS_SESSION_BUS_ADDRESS to point at
the private bus and appends each received call to the log file in argv[1].
"""

import sys

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

IFACE = "org.freedesktop.Notifications"


class Server(dbus.service.Object):
    def __init__(self, bus, log_path):
        super().__init__(bus, "/org/freedesktop/Notifications")
        self._next = 1
        self._log_path = log_path

    @dbus.service.method(IFACE, in_signature="susssasa{sv}i", out_signature="u")
    def Notify(self, app, replaces, icon, summary, body, actions, hints, timeout):
        nid = self._next
        self._next += 1
        with open(self._log_path, "a") as f:
            f.write(f"Notify id={nid} app={app} summary={summary}\n")
        return dbus.UInt32(nid)

    @dbus.service.method(IFACE, in_signature="", out_signature="as")
    def GetCapabilities(self):
        return ["body"]

    @dbus.service.method(IFACE, in_signature="", out_signature="ssss")
    def GetServerInformation(self):
        return ("mx4-verify-fake-notifyd", "verify", "0", "1.2")

    @dbus.service.method(IFACE, in_signature="u", out_signature="")
    def CloseNotification(self, nid):
        pass


def main():
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    name = dbus.service.BusName(IFACE, bus, do_not_queue=True)  # noqa: F841
    Server(bus, sys.argv[1])
    print("fake-notifyd ready", flush=True)
    GLib.MainLoop().run()


if __name__ == "__main__":
    main()
