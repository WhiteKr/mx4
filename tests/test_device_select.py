"""Device selection tests for mx4-haptic-daemon.

Builds a fake /sys/class/hidraw tree and fake /dev nodes, then checks that
HapticSender only ever opens (and writes to) the MX Master 4.

Run: python3 -m unittest discover -s tests
"""

import importlib.machinery
import importlib.util
import os
import sys
import tempfile
import types
import unittest
from unittest import mock

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def load_daemon():
    # The daemon imports dbus/gi/yaml at module level; selection logic needs
    # none of them, so stub whichever are missing.
    for name in ("dbus", "dbus.mainloop", "dbus.mainloop.glib", "gi",
                 "gi.repository", "yaml"):
        if name not in sys.modules:
            try:
                __import__(name)
            except ImportError:
                mod = types.ModuleType(name)
                mod.DBusGMainLoop = None
                mod.GLib = None
                sys.modules[name] = mod
    path = os.path.join(REPO, "mx4-haptic-daemon")
    loader = importlib.machinery.SourceFileLoader("mx4_haptic_daemon", path)
    spec = importlib.util.spec_from_loader(loader.name, loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


daemon = load_daemon()

# Minimal report descriptors.
# Vendor page 0xFF43, Report ID 0x10 (short) and 0x11 (long): HID++ device.
HIDPP_DESC = bytes([
    0x06, 0x43, 0xFF, 0x0A, 0x02, 0x02, 0xA1, 0x01,
    0x85, 0x10, 0x75, 0x08, 0x95, 0x06, 0x15, 0x00, 0x26, 0xFF, 0x00,
    0x09, 0x01, 0x81, 0x00, 0x09, 0x01, 0x91, 0x00,
    0x85, 0x11, 0x95, 0x13, 0x09, 0x02, 0x81, 0x00, 0x09, 0x02, 0x91, 0x00,
    0xC0,
])
# Plain keyboard: Report ID 0x01 only.
KEYBOARD_DESC = bytes([
    0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01,
    0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01,
    0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0xC0,
])


class FakeHidraw:
    def __init__(self, root):
        self.sys = os.path.join(root, "sys")
        self.dev = os.path.join(root, "dev")
        os.makedirs(self.sys)
        os.makedirs(self.dev)

    def add(self, n, hid_id, name, desc):
        d = os.path.join(self.sys, "hidraw%d" % n, "device")
        os.makedirs(d)
        with open(os.path.join(d, "uevent"), "w") as f:
            f.write("DRIVER=hid-generic\nHID_ID=%s\nHID_NAME=%s\n" % (hid_id, name))
        with open(os.path.join(d, "report_descriptor"), "wb") as f:
            f.write(desc)
        node = os.path.join(self.dev, "hidraw%d" % n)
        open(node, "wb").close()
        return node

    def written(self, n):
        with open(os.path.join(self.dev, "hidraw%d" % n), "rb") as f:
            return f.read()


class DeviceSelectTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.fake = FakeHidraw(tmp.name)
        for target, value in (("SYSFS_HIDRAW_DIR", self.fake.sys),
                              ("DEV_DIR", self.fake.dev)):
            p = mock.patch.object(daemon, target, value)
            p.start()
            self.addCleanup(p.stop)
        # Fake nodes are regular files; treat those under the fake /dev as
        # device nodes so the sysfs identity check applies to them.
        dev = self.fake.dev
        p = mock.patch.object(
            daemon, "_is_device_node",
            lambda path: os.path.realpath(path).startswith(dev + os.sep))
        p.start()
        self.addCleanup(p.stop)

    def sender(self, path, device_number=0xFF):
        s = daemon.HapticSender(path, feature_index=11, device_number=device_number)
        self.addCleanup(s.close)
        return s

    def test_configured_path_is_mouse(self):
        mouse = self.fake.add(4, "0005:0000046D:0000B042", "MX Master 4", HIDPP_DESC)
        s = self.sender(mouse)
        self.assertTrue(s.play("SUBTLE_COLLISION"))
        self.assertEqual(s.device_path, mouse)
        self.assertEqual(len(self.fake.written(4)), 20)

    def test_configured_path_is_other_device_scans_to_mouse(self):
        # hidraw4 got renumbered to a keyboard; the mouse is now hidraw7.
        kbd = self.fake.add(4, "0003:00001234:00005678", "Some Keyboard", KEYBOARD_DESC)
        self.fake.add(7, "0005:0000046D:0000B042", "MX Master 4", HIDPP_DESC)
        s = self.sender(kbd)
        self.assertTrue(s.play("SUBTLE_COLLISION"))
        self.assertEqual(s.device_path, os.path.join(self.fake.dev, "hidraw7"))
        self.assertEqual(self.fake.written(4), b"")
        self.assertEqual(len(self.fake.written(7)), 20)

    def test_never_writes_to_other_devices_when_mouse_absent(self):
        self.fake.add(0, "0003:00001234:00005678", "Some Keyboard", KEYBOARD_DESC)
        # A different Logitech HID++ device (e.g. a keyboard) is not the mouse.
        self.fake.add(1, "0005:0000046D:0000B378", "MX Keys S", HIDPP_DESC)
        # Logitech, right name, but no HID++ long report (e.g. mouse input node).
        self.fake.add(2, "0005:0000046D:0000B042", "MX Master 4 Mouse", KEYBOARD_DESC)
        s = self.sender(os.path.join(self.fake.dev, "hidraw9"))
        self.assertFalse(s.play("SUBTLE_COLLISION"))
        for n in (0, 1, 2):
            self.assertEqual(self.fake.written(n), b"", "hidraw%d was written" % n)

    def test_open_raises_when_mouse_absent(self):
        self.fake.add(0, "0003:00001234:00005678", "Some Keyboard", KEYBOARD_DESC)
        s = self.sender(os.path.join(self.fake.dev, "hidraw9"))
        with self.assertRaises(OSError):
            s.open()

    def test_reconnect_after_write_failure_finds_mouse(self):
        mouse = self.fake.add(3, "0005:0000046D:0000B042", "MX Master 4", HIDPP_DESC)
        self.fake.add(1, "0003:00001234:00005678", "Some Keyboard", KEYBOARD_DESC)
        s = self.sender(mouse)
        s.open()
        real_write = os.write
        calls = []

        def flaky_write(fd, data):
            calls.append(fd)
            if len(calls) == 1:
                raise OSError(5, "Input/output error")
            return real_write(fd, data)

        with mock.patch.object(daemon.os, "write", flaky_write):
            self.assertTrue(s.play("SUBTLE_COLLISION"))
        self.assertEqual(len(calls), 2)
        self.assertEqual(self.fake.written(1), b"")
        self.assertEqual(len(self.fake.written(3)), 20)

    def test_receiver_allowed_only_with_slot_device_number(self):
        recv = self.fake.add(2, "0003:0000046D:0000C548", "Logitech USB Receiver", HIDPP_DESC)
        s = self.sender(recv, device_number=0xFF)
        self.assertFalse(s.play("SUBTLE_COLLISION"))
        self.assertEqual(self.fake.written(2), b"")
        s = self.sender(recv, device_number=0x01)
        self.assertTrue(s.play("SUBTLE_COLLISION"))
        self.assertEqual(len(self.fake.written(2)), 20)

    def test_regular_file_outside_dev_is_used_as_is(self):
        # The verify skill points device_path at a plain file.
        path = os.path.join(os.path.dirname(self.fake.dev), "fake-hidraw")
        open(path, "wb").close()
        s = self.sender(path)
        self.assertTrue(s.play("SUBTLE_COLLISION"))
        with open(path, "rb") as f:
            self.assertEqual(len(f.read()), 20)

    def test_report_ids(self):
        self.assertEqual(daemon._report_ids(HIDPP_DESC), {0x10, 0x11})
        self.assertEqual(daemon._report_ids(KEYBOARD_DESC), {0x01})


if __name__ == "__main__":
    unittest.main()
