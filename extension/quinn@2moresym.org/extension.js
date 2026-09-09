import GObject from 'gi://GObject';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

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
        const box = new St.BoxLayout({vertical: true, style_class: 'quinn-popup-box'});
        const inputRow = new St.BoxLayout({style_class: 'quinn-input-row'});
        this._entry = new St.Entry({
            hint_text: 'Ask Quinn…',
            can_focus: true,
            track_hover: true,
            x_expand: true,
        });
        this._voice = new St.Button({
            label: '🎙',
            style_class: 'quinn-voice-button',
            can_focus: true,
        });
        inputRow.add_child(this._entry);
        inputRow.add_child(this._voice);
        this._status = new St.Label({text: 'Ready', style_class: 'quinn-status'});
        box.add_child(inputRow);
        box.add_child(this._status);
        item.add_child(box);
        section.addMenuItem(item);
        this.menu.addMenuItem(section);

        this._entry.clutter_text.connect('activate', () => this._submit());
        this._voice.connect('clicked', () => this._listen());
        this.menu.connect('open-state-changed', (_menu, open) => {
            if (open)
                this._entry.grab_key_focus();
        });

        try {
            this._proxy = Gio.DBusProxy.new_for_bus_sync(
                Gio.BusType.SESSION,
                Gio.DBusProxyFlags.NONE,
                null,
                BUS_NAME,
                OBJECT_PATH,
                INTERFACE,
                null,
            );
        } catch (e) {
            logError(e, 'Quinn D-Bus service is unavailable');
        }
    }

    _submit() {
        const query = this._entry.get_text().trim();
        if (!query || !this._proxy)
            return;
        this._ask(query);
    }

    _ask(query) {
        this._status.set_text('Working…');
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

    _listen() {
        if (!this._proxy)
            return;

        this._status.set_text('Listening…');
        this._voice.reactive = false;
        this._proxy.call(
            'Listen',
            null,
            Gio.DBusCallFlags.NONE,
            -1,
            null,
            (proxy, result) => {
                this._voice.reactive = true;
                try {
                    const reply = proxy.call_finish(result).deep_unpack();
                    const text = reply[0]?.trim() ?? '';
                    if (!text) {
                        this._status.set_text('No speech detected.');
                        return;
                    }
                    this._entry.set_text(text);
                    this._ask(text);
                } catch (e) {
                    logError(e, 'Quinn voice request failed');
                    this._status.set_text('Voice input is unavailable.');
                }
            },
        );
    }
});

export default class QuinnExtension extends Extension {
    enable() {
        this._settings = this.getSettings();
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
