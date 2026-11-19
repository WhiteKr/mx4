from gi.repository import Gio, GLib
XML = """<node><interface name="org.freedesktop.Notifications"><method name="Notify">
<arg type="s"/><arg type="u"/><arg type="s"/><arg type="s"/><arg type="s"/><arg type="as"/><arg type="a{sv}"/><arg type="i"/>
<arg type="u" direction="out"/></method></interface></node>"""
bus = Gio.bus_get_sync(Gio.BusType.SESSION)
bus.register_object('/org/freedesktop/Notifications', Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0],
                    lambda *a: a[-1].return_value(GLib.Variant('(u)', (1,))), None, None)
Gio.bus_own_name_on_connection(bus, 'org.freedesktop.Notifications', 0, None, None)
loop = GLib.MainLoop(); GLib.timeout_add_seconds(6, loop.quit); loop.run()
