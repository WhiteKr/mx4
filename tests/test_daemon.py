import importlib.machinery
import importlib.util
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parent.parent


def load_daemon():
    loader = importlib.machinery.SourceFileLoader("mx4_daemon", str(ROOT / "mx4-haptic-daemon"))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


daemon = load_daemon()


class FakeBus:
    def __init__(self):
        self.signals = {}
        self.filters = []

    def add_signal_receiver(self, handler, signal_name, dbus_interface, path):
        assert dbus_interface == "com.mx4haptic.CursorTracker"
        assert path == "/com/mx4haptic/CursorTracker"
        self.signals[signal_name] = handler

    def add_match_string(self, rule):
        pass

    def add_message_filter(self, handler):
        self.filters.append(handler)

    def emit(self, name, *args):
        self.signals[name](*args)

    def notify(self, summary):
        msg = mock.Mock()
        msg.get_member.return_value = "Notify"
        msg.get_interface.return_value = "org.freedesktop.Notifications"
        msg.get_args_list.return_value = ["app", 0, "", summary, "", [], {}, -1]
        for f in self.filters:
            f(self, msg)


class FakeSender:
    def __init__(self):
        self.played = []

    def play(self, waveform):
        self.played.append(waveform)
        return True


class Clock:
    def __init__(self):
        self.now = 1000.0

    def __call__(self):
        return self.now


def wire(overrides=None):
    cfg = daemon.load_config("/nonexistent")
    for key, value in (overrides or {}).items():
        cfg[key] = {**cfg[key], **value}
    sender, bus, clock = FakeSender(), FakeBus(), Clock()
    with mock.patch.object(daemon.time, "monotonic", clock):
        daemon.setup_triggers(cfg, sender, bus)
    return sender, bus, clock


class PacketTest(unittest.TestCase):
    def test_play_writes_hidpp_long_report(self):
        with tempfile.NamedTemporaryFile() as dev:
            sender = daemon.HapticSender(dev.name, feature_index=11, device_number=0xFF)
            self.assertTrue(sender.play("HAPPY_ALERT"))
            sender.close()
            data = Path(dev.name).read_bytes()
        self.assertEqual(data, bytes([0x11, 0xFF, 11, 0x41, 0x05]) + bytes(15))

    def test_unknown_waveform_writes_nothing(self):
        with tempfile.NamedTemporaryFile() as dev:
            sender = daemon.HapticSender(dev.name, feature_index=11, device_number=0xFF)
            self.assertFalse(sender.play("NOPE"))
            self.assertEqual(Path(dev.name).read_bytes(), b"")


class ConfigTest(unittest.TestCase):
    def test_user_values_merge_over_defaults(self):
        with tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False) as f:
            f.write("feature_index: 7\nnotifications:\n  cooldown_s: 9\n")
        try:
            cfg = daemon.load_config(f.name)
        finally:
            os.unlink(f.name)
        self.assertEqual(cfg["feature_index"], 7)
        self.assertEqual(cfg["notifications"]["cooldown_s"], 9)
        self.assertEqual(cfg["notifications"]["waveform"], "HAPPY_ALERT")
        self.assertTrue(cfg["focus_change"]["enabled"])


class TriggerTest(unittest.TestCase):
    def test_each_signal_plays_its_waveform(self):
        sender, bus, clock = wire({"notifications": {"waveform": "KNOCK"}})
        with mock.patch.object(daemon.time, "monotonic", clock):
            bus.emit("BoundaryCrossed", 0, 1, 1920, 500)
            bus.emit("WorkspaceSwitched", 0, 1)
            bus.emit("WindowSnapped", "left", 0)
            bus.emit("FocusChanged", "firefox")
            bus.notify("hello")
        self.assertEqual(sender.played, ["SUBTLE_COLLISION"] * 4 + ["KNOCK"])

    def test_throttle_drops_events_inside_window(self):
        sender, bus, clock = wire()
        with mock.patch.object(daemon.time, "monotonic", clock):
            bus.emit("FocusChanged", "a")
            clock.now += 0.1
            bus.emit("FocusChanged", "b")
            clock.now += 0.3
            bus.emit("FocusChanged", "c")
            for _ in range(3):
                bus.emit("WorkspaceSwitched", 0, 1)
        self.assertEqual(len(sender.played), 5)

    def test_disabled_trigger_is_not_subscribed(self):
        _, bus, _ = wire({"window_snap": {"enabled": False}, "notifications": {"enabled": False}})
        self.assertNotIn("WindowSnapped", bus.signals)
        self.assertEqual(bus.filters, [])


if __name__ == "__main__":
    unittest.main()
