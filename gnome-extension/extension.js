import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';

const DBUS_IFACE = `
<node>
  <interface name="com.mx4haptic.CursorTracker">
    <signal name="BoundaryCrossed">
      <arg type="i" name="from_monitor"/>
      <arg type="i" name="to_monitor"/>
      <arg type="i" name="x"/>
      <arg type="i" name="y"/>
    </signal>
    <method name="GetPosition">
      <arg type="i" direction="out" name="x"/>
      <arg type="i" direction="out" name="y"/>
      <arg type="i" direction="out" name="monitor"/>
    </method>
  </interface>
</node>`;

export default class Mx4CursorTracker {
    _pollId = 0;
    _currentMonitor = -1;
    _dbusImpl = null;
    _rect = null;

    enable() {
        const nodeInfo = Gio.DBusNodeInfo.new_for_xml(DBUS_IFACE);
        this._dbusImpl = Gio.DBusExportedObject.wrapJSObject(
            nodeInfo.interfaces[0], this);
        this._dbusImpl.export(Gio.DBus.session, '/com/mx4haptic/CursorTracker');

        this._rect = new Meta.Rectangle({ x: 0, y: 0, width: 1, height: 1 });

        this._pollId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 50, () => {
            this._checkCursor();
            return GLib.SOURCE_CONTINUE;
        });

        log('[mx4-cursor-tracker] enabled');
    }

    disable() {
        if (this._pollId) {
            GLib.source_remove(this._pollId);
            this._pollId = 0;
        }
        if (this._dbusImpl) {
            this._dbusImpl.unexport();
            this._dbusImpl = null;
        }
        this._currentMonitor = -1;
        this._rect = null;
        log('[mx4-cursor-tracker] disabled');
    }

    _getPointerMonitor() {
        const [x, y] = global.get_pointer();
        this._rect.x = x;
        this._rect.y = y;
        const monitorIdx = global.display.get_monitor_index_for_rect(this._rect);
        return [x, y, monitorIdx];
    }

    _checkCursor() {
        const [x, y, monitorIdx] = this._getPointerMonitor();

        if (this._currentMonitor === -1) {
            this._currentMonitor = monitorIdx;
            return;
        }

        if (monitorIdx !== this._currentMonitor) {
            const prev = this._currentMonitor;
            this._currentMonitor = monitorIdx;
            if (this._dbusImpl) {
                this._dbusImpl.emit_signal('BoundaryCrossed',
                    new GLib.Variant('(iiii)', [prev, monitorIdx, x, y]));
            }
        }
    }

    GetPosition() {
        return this._getPointerMonitor();
    }
}
