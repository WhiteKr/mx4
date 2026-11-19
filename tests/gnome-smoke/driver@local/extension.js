import Gio from 'gi://Gio';
import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
const XML = `<node><interface name="smoke.Driver">
<method name="Warp"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="NextWorkspace"/><method name="Maximize"/></interface></node>`;
export default class Driver extends Extension {
  enable() {
    this._impl = Gio.DBusExportedObject.wrapJSObject(Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0], this);
    this._impl.export(Gio.DBus.session, '/smoke/Driver');
    Main.layoutManager.connect('startup-complete', () => Main.overview.hide());
    Main.overview.hide();
  }
  disable() { this._impl.unexport(); }
  Warp(x, y) { Clutter.get_default_backend().get_default_seat().warp_pointer(x, y); }
  Maximize() { global.display.focus_window.maximize(Meta.MaximizeFlags.BOTH); }
  NextWorkspace() {
    const m = global.workspace_manager;
    if (m.n_workspaces < 2) m.append_new_workspace(false, global.get_current_time());
    m.get_workspace_by_index((m.get_active_workspace_index() + 1) % m.n_workspaces).activate(global.get_current_time());
  }
}
