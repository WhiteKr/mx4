import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Mtk from 'gi://Mtk';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const SNAP_SETTLE_MS = 100;

const DBUS_IFACE = `
<node>
  <interface name="com.mx4haptic.CursorTracker">
    <signal name="BoundaryCrossed">
      <arg type="i" name="from_monitor"/>
      <arg type="i" name="to_monitor"/>
      <arg type="i" name="x"/>
      <arg type="i" name="y"/>
    </signal>
    <signal name="WorkspaceSwitched">
      <arg type="i" name="from_index"/>
      <arg type="i" name="to_index"/>
    </signal>
    <signal name="WindowSnapped">
      <arg type="s" name="snap_type"/>
      <arg type="i" name="monitor_index"/>
    </signal>
    <signal name="FocusChanged">
      <arg type="s" name="window_name"/>
    </signal>
    <method name="GetPosition">
      <arg type="i" direction="out" name="x"/>
      <arg type="i" direction="out" name="y"/>
      <arg type="i" direction="out" name="monitor"/>
    </method>
  </interface>
</node>`;

export default class Mx4CursorTracker extends Extension {
    _pollId = 0;
    _currentMonitor = -1;
    _dbusImpl = null;
    _rect = null;
    _currentWorkspace = -1;
    _focusWindowId = 0;
    _wsChangedId = 0;
    _trackedWin = null;
    _trackedWinIds = [];
    _lastSnapState = null;
    _lastFocusWindow = null;
    _snapSettleId = 0;

    enable() {
        const nodeInfo = Gio.DBusNodeInfo.new_for_xml(DBUS_IFACE);
        this._dbusImpl = Gio.DBusExportedObject.wrapJSObject(
            nodeInfo.interfaces[0], this);
        this._dbusImpl.export(Gio.DBus.session, '/com/mx4haptic/CursorTracker');

        this._rect = new Mtk.Rectangle({x: 0, y: 0, width: 1, height: 1});

        this._pollId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 50, () => {
            this._checkCursor();
            return GLib.SOURCE_CONTINUE;
        });

        const wsMgr = global.workspace_manager;
        this._currentWorkspace = wsMgr.get_active_workspace_index();
        this._wsChangedId = wsMgr.connect('active-workspace-changed', () => {
            const newIdx = wsMgr.get_active_workspace_index();
            const oldIdx = this._currentWorkspace;
            if (newIdx !== oldIdx) {
                this._currentWorkspace = newIdx;
                this._dbusImpl?.emit_signal('WorkspaceSwitched',
                    new GLib.Variant('(ii)', [oldIdx, newIdx]));
            }
        });

        this._focusWindowId = global.display.connect('notify::focus-window', () => {
            this._onFocusChanged();
        });

        console.log('[mx4-cursor-tracker] enabled');
    }

    disable() {
        if (this._pollId) {
            GLib.source_remove(this._pollId);
            this._pollId = 0;
        }
        if (this._snapSettleId) {
            GLib.source_remove(this._snapSettleId);
            this._snapSettleId = 0;
        }
        if (this._wsChangedId) {
            global.workspace_manager.disconnect(this._wsChangedId);
            this._wsChangedId = 0;
        }
        if (this._focusWindowId) {
            global.display.disconnect(this._focusWindowId);
            this._focusWindowId = 0;
        }
        this._disconnectTrackedWindow();
        if (this._dbusImpl) {
            this._dbusImpl.unexport();
            this._dbusImpl = null;
        }
        this._currentMonitor = -1;
        this._currentWorkspace = -1;
        this._lastSnapState = null;
        this._lastFocusWindow = null;
        this._trackedWin = null;
        this._rect = null;
        console.log('[mx4-cursor-tracker] disabled');
    }

    _disconnectTrackedWindow() {
        if (this._trackedWin) {
            for (const id of this._trackedWinIds) {
                try { this._trackedWin.disconnect(id); } catch (e) {}
            }
        }
        this._trackedWin = null;
        this._trackedWinIds = [];
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
            this._dbusImpl?.emit_signal('BoundaryCrossed',
                new GLib.Variant('(iiii)', [prev, monitorIdx, x, y]));
        }
    }

    _onFocusChanged() {
        const win = global.display.focus_window;
        if (!win || win === this._lastFocusWindow) return;

        this._lastFocusWindow = win;

        const name = win.get_wm_class() || win.get_title() || 'unknown';
        this._dbusImpl?.emit_signal('FocusChanged',
            new GLib.Variant('(s)', [name]));

        this._disconnectTrackedWindow();
        this._lastSnapState = this._getSnapState(win);
        this._trackedWin = win;

        try {
            const sizeId = win.connect('size-changed',
                () => this._onWindowSizeChanged(win));
            this._trackedWinIds = [sizeId];
        } catch (e) {
            this._trackedWin = null;
            this._trackedWinIds = [];
        }
    }

    _getSnapState(win) {
        try {
            const rect = win.get_frame_rect();
            const monIdx = win.get_monitor();
            const workarea = win.get_work_area_for_monitor(monIdx);
            const halfW = Math.floor(workarea.width / 2);
            const tol = 10;

            if (Math.abs(rect.width - workarea.width) < tol &&
                Math.abs(rect.height - workarea.height) < tol) {
                return 'maximize';
            }
            if (Math.abs(rect.width - halfW) < tol &&
                Math.abs(rect.height - workarea.height) < tol) {
                if (Math.abs(rect.x - workarea.x) < tol) return 'left';
                if (Math.abs(rect.x - (workarea.x + halfW)) < tol) return 'right';
            }
        } catch (e) {}
        return null;
    }

    _onWindowSizeChanged(win) {
        if (this._snapSettleId) {
            GLib.source_remove(this._snapSettleId);
        }
        this._snapSettleId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SNAP_SETTLE_MS, () => {
            this._snapSettleId = 0;
            const newState = this._getSnapState(win);
            if (newState && newState !== this._lastSnapState) {
                this._dbusImpl?.emit_signal('WindowSnapped',
                    new GLib.Variant('(si)', [newState, win.get_monitor()]));
            }
            this._lastSnapState = newState;
            return GLib.SOURCE_REMOVE;
        });
    }

    GetPosition() {
        return this._getPointerMonitor();
    }
}
