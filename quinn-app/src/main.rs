use gio::prelude::*;
use gtk4::prelude::*;
use gtk4::{glib, Application, ApplicationWindow, Button, Entry, Label, Orientation, ScrolledWindow, TextBuffer, TextView};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::rc::Rc;
use zbus::proxy;

const APP_ID: &str = "org.quinn.AssistantApp";
const BUS_NAME: &str = "org.quinn.Assistant";
const OBJECT_PATH: &str = "/org/quinn/Assistant";
const VOICE_DIR: &str = ".local/share/quinn/voices/female";

#[proxy(
    interface = "org.quinn.Assistant",
    default_service = "org.quinn.Assistant",
    default_path = "/org/quinn/Assistant"
)]
trait QuinnAssistant {
    #[zbus(name = "Ask")]
    fn ask(&self, query: &str) -> zbus::Result<(String, String)>;

    #[zbus(name = "Listen")]
    fn listen(&self) -> zbus::Result<String>;

    #[zbus(property, name = "Ready")]
    fn ready(&self) -> zbus::Result<bool>;
}

fn user_data_path(relative: &str) -> PathBuf {
    glib::user_data_dir().join(relative)
}

fn play_clip(filename: &str) -> bool {
    let path = user_data_path(&format!("{VOICE_DIR}/{filename}"));
    if !path.is_file() {
        return false;
    }

    for player in ["pw-play", "paplay", "aplay"] {
        if Command::new(player).arg(&path).spawn().is_ok() {
            return true;
        }
    }
    false
}

fn greeting_clip() -> &'static str {
    match chrono_like_hour() {
        5..=11 => "Good_Morning.wav",
        12..=17 => "Good_Afternoon.wav",
        18..=21 => "Good_Evening.wav",
        _ => "Good_Night.wav",
    }
}

fn chrono_like_hour() -> u32 {
    glib::DateTime::now_local()
        .ok()
        .and_then(|time| time.hour())
        .unwrap_or(12) as u32
}

fn start_daemon() -> Option<Child> {
    let binary = dirs_like_home().join(".local/bin/quinn-daemon");
    if !binary.is_file() {
        return None;
    }
    Command::new(binary).spawn().ok()
}

fn dirs_like_home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn main() {
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| {
        let daemon = Rc::new(RefCell::new(start_daemon()));
        let connected = Rc::new(Cell::new(false));

        let window = ApplicationWindow::builder()
            .application(app)
            .title("Quinn")
            .default_width(520)
            .default_height(420)
            .build();

        let root = gtk4::Box::new(Orientation::Vertical, 12);
        root.set_margin_top(18);
        root.set_margin_bottom(18);
        root.set_margin_start(18);
        root.set_margin_end(18);

        let title = Label::new(Some("Quinn"));
        title.add_css_class("title-1");
        root.append(&title);

        let status = Label::new(Some("Starting Quinn…"));
        status.set_xalign(0.0);
        root.append(&status);

        let output = TextView::new();
        output.set_editable(false);
        output.set_wrap_mode(gtk4::WrapMode::WordChar);
        let output_buffer = TextBuffer::new(None);
        output.set_buffer(Some(&output_buffer));
        let scroll = ScrolledWindow::builder().child(&output).vexpand(true).build();
        root.append(&scroll);

        let entry = Entry::builder().placeholder_text("Ask Quinn…").hexpand(true).build();
        let send = Button::with_label("Send");
        let listen = Button::with_label("🎙");
        let row = gtk4::Box::new(Orientation::Horizontal, 8);
        row.append(&entry);
        row.append(&listen);
        row.append(&send);
        root.append(&row);

        window.set_child(Some(&root));
        window.present();

        play_clip(greeting_clip());

        let status_clone = status.clone();
        let connected_clone = connected.clone();
        let daemon_clone = daemon.clone();
        glib::MainContext::default().spawn_local(async move {
            match zbus::Connection::session().await {
                Ok(connection) => match QuinnAssistantProxy::builder(&connection)
                    .destination(BUS_NAME)
                    .and_then(|b| b.path(OBJECT_PATH))
                    .and_then(|b| b.build().await)
                {
                    Ok(proxy) => match proxy.ready().await {
                        Ok(true) => {
                            connected_clone.set(true);
                            status_clone.set_text("Ready");
                            drop(daemon_clone);
                        }
                        _ => status_clone.set_text("Daemon is starting…"),
                    },
                    Err(_) => status_clone.set_text("Waiting for Quinn daemon…"),
                },
                Err(_) => status_clone.set_text("Could not connect to session bus."),
            }
        });

        let submit = {
            let entry = entry.clone();
            let output_buffer = output_buffer.clone();
            let status = status.clone();
            let connected = connected.clone();
            move || {
                let query = entry.text().trim().to_string();
                if query.is_empty() || !connected.get() {
                    return;
                }
                status.set_text("Thinking…");
                let entry = entry.clone();
                let output_buffer = output_buffer.clone();
                let status = status.clone();
                glib::MainContext::default().spawn_local(async move {
                    match zbus::Connection::session().await {
                        Ok(connection) => {
                            match QuinnAssistantProxy::builder(&connection)
                                .destination(BUS_NAME)
                                .and_then(|b| b.path(OBJECT_PATH))
                                .and_then(|b| b.build().await)
                            {
                                Ok(proxy) => match proxy.ask(&query).await {
                                    Ok((response, _)) => {
                                        output_buffer.set_text(&format!("You: {query}\n\nQuinn: {response}"));
                                        status.set_text("Ready");
                                        entry.set_text("");
                                    }
                                    Err(_) => status.set_text("Quinn could not process that request."),
                                },
                                Err(_) => status.set_text("Quinn daemon is unavailable."),
                            }
                        }
                        Err(_) => status.set_text("Session bus unavailable."),
                    }
                });
            }
        };

        let submit_send = submit.clone();
        send.connect_clicked(move |_| submit_send());
        entry.connect_activate(move |_| submit());

        let entry_voice = entry.clone();
        let output_voice = output_buffer.clone();
        let status_voice = status.clone();
        let connected_voice = connected.clone();
        listen.connect_clicked(move |_| {
            if !connected_voice.get() {
                return;
            }
            status_voice.set_text("Listening…");
            play_clip("Hmm.wav");
            let entry = entry_voice.clone();
            let output = output_voice.clone();
            let status = status_voice.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(350), move || {
                glib::MainContext::default().spawn_local(async move {
                    match zbus::Connection::session().await {
                        Ok(connection) => {
                            match QuinnAssistantProxy::builder(&connection)
                                .destination(BUS_NAME)
                                .and_then(|b| b.path(OBJECT_PATH))
                                .and_then(|b| b.build().await)
                            {
                                Ok(proxy) => match proxy.listen().await {
                                    Ok(text) if !text.trim().is_empty() => {
                                        entry.set_text(&text);
                                        status.set_text("Heard you — working…");
                                        match proxy.ask(&text).await {
                                            Ok((response, _)) => {
                                                output.set_text(&format!("You: {text}\n\nQuinn: {response}"));
                                                status.set_text("Ready");
                                            }
                                            Err(_) => status.set_text("Quinn could not process that request."),
                                        }
                                    }
                                    Ok(_) => status.set_text("No speech detected."),
                                    Err(_) => status.set_text("Voice input is unavailable."),
                                },
                                Err(_) => status.set_text("Quinn daemon is unavailable."),
                            }
                        }
                        Err(_) => status.set_text("Session bus unavailable."),
                    }
                });
            });
        });

        let daemon_for_shutdown = daemon.clone();
        app.connect_shutdown(move |_| {
            if let Some(mut child) = daemon_for_shutdown.borrow_mut().take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        });
    });

    app.run();
}
