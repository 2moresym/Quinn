import GObject from 'gi://GObject';
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import * as ExtensionUtils from 'resource:///org/gnome/shell/misc/extensionUtils.js';

const BUS_NAME = 'org.quinn.Assistant';
const OBJECT_PATH = '/org/quinn/Assistant';
const INTERFACE = 'org.quinn.Assistant';

const QuinnButton = GObject.registerClass(
class QuinnButton extends PanelMenu.Button {
    _init() {
        super._init(0.0, 'Quinn');
        this._proxy = null;
        this._entry = null;
        this._status = null;

        const icon = new St.Icon({
            icon_name: 'system-search-symbolic',
            style_class: 'system-status-icon',
        });
        this.add_child(icon);

        const section = new PopupMenu.PopupMenuSection();
        const item = new PopupMenu.PopupBaseMenuItem({
            activate: false,
            can_focus: false,
        });
        const box = new St.BoxLayout({ vertical: true, style_class: 'quinn-popup-box' });
        this._entry = new St.Entry({
            hint_text: 'Ask Quinn…',
            can_focus: true,
            track_hover: true,
        });
        this._status = new St.Label({ text: 'Ready', style_class: 'quinn-status' });
        box.add_child(this._entry);
        box.add_child(this._status);
        item.add_child(box);
        section.addMenuItem(item);
        this.menu.addMenuItem(section);

        this._entry.clutter_text.connect('activate', () => this._submit());
        this.menu.connect('open-state-changed', (_menu, open) => {
            if (open)
                this._entry.grab_key_focus();
        });

        this._proxy = Gio.DBusProxy.new_for_bus_sync(
            Gio.BusType.SESSION,
            Gio.DBusProxyFlags.NONE,
            null,
            BUS_NAME,
            OBJECT_PATH,
            INTERFACE,
            null,
        );
    }

    _submit() {
        const query = this._entry.get_text().trim();
        if (!query || !this._proxy)
            return;

        this._status.set_text('Working…');
        this._entry.set_text('');
        this._proxy.call(
            'Ask',
            new GLib.Variant('(s)', [query]),
            Gio.DBusCallFlags.NONE,
            -1,
            null,
            (proxy, result) => {
                try {
                    const reply = proxy.call_finish(result).deep_unpack();
                    this._status.set_text(reply[0]);
                } catch (e) {
                    logError(e, 'Quinn D-Bus request failed');
                    this._status.set_text('Quinn is unavailable.');
                }
            },
        );
    }
});

export default class QuinnExtension {
    enable() {
        this._settings = ExtensionUtils.getSettings();
        this._button = new QuinnButton();
        Main.panel.addToStatusArea('quinn', this._button);
        Main.wm.addKeybinding(
            'toggle-quinn',
            this._settings,
            Meta.KeyBindingFlags.NONE,
            Shell.ActionMode.ALL,
            () => this._button.menu.toggle(),
        );
    }

    disable() {
        Main.wm.removeKeybinding('toggle-quinn');
        this._button?.destroy();
        this._settings = null;
        this._button = null;
    }
}
