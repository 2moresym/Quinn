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
const RETRY_MS = 1500;

const QuinnButton = GObject.registerClass(
class QuinnButton extends PanelMenu.Button {
    _init() {
        super._init(0.0, 'Quinn');
        this._proxy = null;
        this._entry = null;
        this._voice = null;
        this._status = null;
        this._retrySource = 0;
        this._signalId = 0;
        this._busy = false;

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
        this._status = new St.Label({text: 'Connecting…', style_class: 'quinn-status'});
        box.add_child(inputRow);
        box.add_child(this._status);
        item.add_child(box);
        section.addMenuItem(item);
        this.menu.addMenuItem(section);

        this._entry.clutter_text.connect('activate', () => this._submit());
        this._voice.connect('clicked', () => this._listen());
        this.menu.connect('open-state-changed', (_menu, open) => {
            if (open && !this._busy)
                this._entry.grab_key_focus();
        });

        this._connectProxy();
    }

    _connectProxy() {
        this._disconnectProxy();
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
            this._signalId = this._proxy.connectSignal('ToolExecuted', (_proxy, _sender, parameters) => {
                const [name, _args, result] = parameters.deep_unpack();
                this._status.set_text(`Executed ${name}: ${result}`);
                this._busy = false;
                this._voice.reactive = true;
                this._entry.reactive = true;
            });
            this._status.set_text('Ready');
        } catch (e) {
            this._proxy = null;
            this._status.set_text('Waiting for Quinn daemon…');
            this._scheduleReconnect();
            logError(e, 'Quinn D-Bus service is unavailable');
        }
    }

    _disconnectProxy() {
        if (this._proxy && this._signalId) {
            this._proxy.disconnectSignal(this._signalId);
            this._signalId = 0;
        }
        this._proxy = null;
    }

    _scheduleReconnect() {
        if (this._retrySource)
            return;
        this._retrySource = GLib.timeout_add(GLib.PRIORITY_DEFAULT, RETRY_MS, () => {
            this._retrySource = 0;
            this._connectProxy();
            return GLib.SOURCE_REMOVE;
        });
    }

    _setBusy(status) {
        this._busy = true;
        this._status.set_text(status);
        this._voice.reactive = false;
        this._entry.reactive = false;
    }

    _finishBusy(status) {
        this._busy = false;
        this._status.set_text(status);
        this._voice.reactive = true;
        this._entry.reactive = true;
    }

    _submit() {
        const query = this._entry.get_text().trim();
        if (!query || this._busy)
            return;
        if (!this._proxy) {
            this._status.set_text('Quinn daemon is not ready.');
            this._scheduleReconnect();
            return;
        }
        this._ask(query);
    }

    _ask(query) {
        this._setBusy('Working…');
        this._proxy.call(
            'Ask',
            new GLib.Variant('(s)', [query]),
            Gio.DBusCallFlags.NONE,
            -1,
            null,
            (proxy, result) => {
                try {
                    const reply = proxy.call_finish(result).deep_unpack();
                    this._finishBusy(reply[0] || 'Done.');
                } catch (e) {
                    this._finishBusy('Quinn is unavailable.');
                    logError(e, 'Quinn D-Bus request failed');
                    this._connectProxy();
                }
            },
        );
    }

    _listen() {
        if (!this._proxy || this._busy)
            return;

        this._setBusy('Listening…');
        this._proxy.call(
            'Listen',
            null,
            Gio.DBusCallFlags.NONE,
            -1,
            null,
            (proxy, result) => {
                try {
                    const reply = proxy.call_finish(result).deep_unpack();
                    const text = reply[0]?.trim() ?? '';
                    if (!text) {
                        this._finishBusy('No speech detected.');
                        return;
                    }
                    this._entry.set_text(text);
                    this._ask(text);
                } catch (e) {
                    this._finishBusy('Voice input is unavailable.');
                    logError(e, 'Quinn voice request failed');
                    this._connectProxy();
                }
            },
        );
    }

    destroy() {
        if (this._retrySource) {
            GLib.source_remove(this._retrySource);
            this._retrySource = 0;
        }
        this._disconnectProxy();
        super.destroy();
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
