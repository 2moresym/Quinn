use std::{
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use needle_infer::v2_engine::V2Engine;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::json;
use tracing::{info, warn};
use zbus::{fdo, interface, object_server::SignalEmitter};

use crate::{
    app_classifier::AppClassifier,
    apps::AppCatalog,
    audio::{VoiceClip, VoicePlayer},
    commands::split_utterance,
    reminders::{Reminder, ReminderStore},
    tools::{self, ToolCall, TOOL_SCHEMAS},
    voice::VoiceEngine,
};

const CONFIDENCE_THRESHOLD: f32 = 0.70;
const APP_EVENT_DEBOUNCE: Duration = Duration::from_millis(250);
const REMINDER_TOOL_SCHEMA: &str = r#"[{"name":"create_reminder","description":"Create a persistent desktop reminder. Use delay_seconds for when it should fire relative to now.","parameters":{"type":"object","properties":{"text":{"type":"string"},"delay_seconds":{"type":"integer","minimum":1,"maximum":315360000}},"required":["text","delay_seconds"]}}]"#;
const REMINDER_REMOVE_TOOL_SCHEMA: &str = r#"[{"name":"remove_reminder","description":"Remove a persistent Quinn reminder by its numeric reminder ID.","parameters":{"type":"object","properties":{"id":{"type":"integer","minimum":1}},"required":["id"]}}]"#;

pub struct QuinnDaemon {
    engine: Arc<V2Engine>,
    apps: Arc<RwLock<AppCatalog>>,
    classifier: Arc<RwLock<AppClassifier>>,
    _app_watcher: Mutex<Option<RecommendedWatcher>>,
    reminders: ReminderStore,
    voice: Option<Arc<VoiceEngine>>,
    voice_player: Option<Arc<VoicePlayer>>,
}

impl QuinnDaemon {
    pub fn new(
        engine: Arc<V2Engine>,
        apps: Arc<RwLock<AppCatalog>>,
        classifier: Arc<RwLock<AppClassifier>>,
        voice: Option<Arc<VoiceEngine>>,
        voice_player: Option<Arc<VoicePlayer>>,
    ) -> Self {
        let app_watcher = create_app_watcher(Arc::clone(&apps), Arc::clone(&classifier));
        let reminders = ReminderStore::load();
        reminders.start_scheduler(Arc::new(move |reminder: Reminder| {
            let body = reminder.text;
            let _ = std::process::Command::new("notify-send")
                .args(["Quinn Reminder", &body])
                .spawn();
        }));
        Self {
            engine,
            apps,
            classifier,
            _app_watcher: Mutex::new(app_watcher),
            reminders,
            voice,
            voice_player,
        }
    }

    fn play_response(&self, clip: VoiceClip) {
        let Some(player) = &self.voice_player else {
            return;
        };
        if let Err(error) = player.play(clip) {
            warn!(%error, ?clip, "failed to play Quinn response voice clip");
        }
    }

    fn tool_schemas(&self) -> String {
        [
            TOOL_SCHEMAS.trim_start_matches('[').trim_end_matches(']'),
            REMINDER_TOOL_SCHEMA
                .trim_start_matches('[')
                .trim_end_matches(']'),
            REMINDER_REMOVE_TOOL_SCHEMA
                .trim_start_matches('[')
                .trim_end_matches(']'),
        ]
        .join(",")
        .pipe(|body| format!("[{body}]"))
    }

    fn execute_fragment(&self, fragment: &str) -> (String, Option<(ToolCall, String)>) {
        let schemas = self.tool_schemas();
        let result = self.engine.run(fragment, &schemas);
        if let Some(err) = result.error() {
            self.play_response(VoiceClip::Sorry);
            return (format!("I couldn't process that command: {err}"), None);
        }

        let Some(raw_call) = result.tool_call.as_deref() else {
            self.play_response(VoiceClip::Sorry);
            return (
                "I didn't find an action for that command.".to_string(),
                None,
            );
        };

        let confidence = self
            .engine
            .confidence_for(fragment, &schemas, &result.text)
            .unwrap_or(0.0);
        if confidence < CONFIDENCE_THRESHOLD {
            warn!(
                query = fragment,
                confidence, "tool call rejected by confidence gate"
            );
            self.play_response(VoiceClip::Sorry);
            return (
                "I'm not confident enough to run that command.".to_string(),
                None,
            );
        }

        let call = match tools::parse_tool_call(raw_call) {
            Ok(call) => call,
            Err(e) => {
                self.play_response(VoiceClip::Sorry);
                return (format!("I couldn't understand the tool call: {e}"), None);
            }
        };

        match call.name.as_str() {
            "create_reminder" => self.create_reminder(&call),
            "remove_reminder" => self.remove_reminder(&call),
            _ => match tools::execute(&call, &self.apps, &self.classifier) {
                Ok(message) => {
                    self.play_response(VoiceClip::Done);
                    ("Done.".to_string(), Some((call, message)))
                }
                Err(message) => {
                    self.play_response(VoiceClip::Sorry);
                    (
                        format!("I couldn't complete that: {message}"),
                        Some((call, message)),
                    )
                }
            },
        }
    }

    fn create_reminder(&self, call: &ToolCall) -> (String, Option<(ToolCall, String)>) {
        let text = call
            .arguments
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let delay = call
            .arguments
            .get("delay_seconds")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let when = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().saturating_add(delay))
            .unwrap_or(0);

        match self.reminders.create(text, when) {
            Ok(reminder) => {
                self.play_response(VoiceClip::TimerSet);
                (
                    format!("Reminder set for {} seconds from now.", delay),
                    Some((call.clone(), format!("created reminder #{}", reminder.id))),
                )
            }
            Err(error) => {
                self.play_response(VoiceClip::Sorry);
                (
                    format!("I couldn't create that reminder: {error}"),
                    Some((call.clone(), error)),
                )
            }
        }
    }

    fn remove_reminder(&self, call: &ToolCall) -> (String, Option<(ToolCall, String)>) {
        let id = call
            .arguments
            .get("id")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);

        match self.reminders.remove(id) {
            Ok(reminder) => {
                self.play_response(VoiceClip::TimerRemoved);
                (
                    format!("Removed reminder #{}.", reminder.id),
                    Some((call.clone(), format!("removed reminder #{}", reminder.id))),
                )
            }
            Err(error) => {
                self.play_response(VoiceClip::Sorry);
                (
                    format!("I couldn't remove that reminder: {error}"),
                    Some((call.clone(), error)),
                )
            }
        }
    }
}

fn create_app_watcher(
    apps: Arc<RwLock<AppCatalog>>,
    classifier: Arc<RwLock<AppClassifier>>,
) -> Option<RecommendedWatcher> {
    let debounce = Arc::new(Mutex::new(Instant::now() - APP_EVENT_DEBOUNCE));
    let callback_apps = Arc::clone(&apps);
    let callback_classifier = Arc::clone(&classifier);
    let callback_debounce = Arc::clone(&debounce);

    let mut watcher = match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        let Ok(event) = result else {
            warn!("application desktop-file watcher reported an error");
            return;
        };

        if !matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        ) {
            return;
        }

        let Ok(mut last_event) = callback_debounce.lock() else {
            warn!("application watcher debounce lock is poisoned");
            return;
        };
        if last_event.elapsed() < APP_EVENT_DEBOUNCE {
            return;
        }
        *last_event = Instant::now();
        drop(last_event);

        refresh_app_intelligence(&callback_apps, &callback_classifier);
    }) {
        Ok(watcher) => watcher,
        Err(error) => {
            warn!(%error, "failed to create application desktop-file watcher");
            return None;
        }
    };

    let mut watched = 0usize;
    for path in application_watch_paths() {
        if !path.is_dir() {
            continue;
        }
        match watcher.watch(&path, RecursiveMode::NonRecursive) {
            Ok(()) => watched += 1,
            Err(error) => {
                warn!(path = %path.display(), %error, "failed to watch application directory")
            }
        }
    }

    if watched == 0 {
        warn!("no application directories could be watched");
        return None;
    }

    info!(
        directories = watched,
        "live application intelligence watcher ready"
    );
    Some(watcher)
}

fn application_watch_paths() -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(3);
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(".local/share/applications"));
    }
    paths.push(PathBuf::from("/usr/local/share/applications"));
    paths.push(PathBuf::from("/usr/share/applications"));
    paths.push(PathBuf::from("/usr/share/gnome/applications"));
    paths
}

fn refresh_app_intelligence(apps: &RwLock<AppCatalog>, classifier: &RwLock<AppClassifier>) {
    let old_count = apps.read().map(|catalog| catalog.len()).unwrap_or(0);
    let old_classified = classifier
        .read()
        .map(|classifier| classifier.len())
        .unwrap_or(0);

    let Ok(mut apps_guard) = apps.write() else {
        warn!("application catalog lock is poisoned");
        return;
    };
    let Ok(mut classifier_guard) = classifier.write() else {
        warn!("application classifier lock is poisoned");
        return;
    };

    apps_guard.refresh();
    classifier_guard.refresh();

    let new_count = apps_guard.len();
    let new_classified = classifier_guard.len();
    if old_count != new_count || old_classified != new_classified {
        info!(
            old_count,
            new_count,
            old_classified,
            new_classified,
            "refreshed application intelligence"
        );
    }
}

#[interface(name = "org.quinn.Assistant", spawn = true)]
impl QuinnDaemon {
    #[zbus(property, name = "Ready")]
    fn ready(&self) -> bool {
        true
    }

    #[zbus(out_args("response", "tool_calls"))]
    async fn ask(
        &self,
        query: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<(String, String)> {
        let query = query.trim();
        if query.is_empty() {
            return Ok((
                "Tell me what you want me to do.".to_string(),
                "[]".to_string(),
            ));
        }

        let mut responses = Vec::new();
        let mut executed = Vec::new();

        for fragment in split_utterance(query) {
            let (response, event) = self.execute_fragment(&fragment);
            responses.push(response);
            if let Some((call, result)) = event {
                let event_json = tools::event_json(&call, &result);
                if let Err(e) = emitter
                    .tool_executed(&call.name, &call.arguments.to_string(), &result)
                    .await
                {
                    warn!(error = %e, "failed to emit ToolExecuted signal");
                }
                executed.push(event_json);
            }
        }

        let response = if responses.len() == 1 {
            responses.remove(0)
        } else {
            responses.join(" ")
        };
        let tool_calls = json!(executed).to_string();
        let app_count = self.apps.read().map(|catalog| catalog.len()).unwrap_or(0);
        let classified_app_count = self
            .classifier
            .read()
            .map(|classifier| classifier.len())
            .unwrap_or(0);
        info!(query, app_count, classified_app_count, "handled request");
        Ok((response, tool_calls))
    }

    #[zbus(out_args("text"))]
    async fn listen(&self) -> fdo::Result<String> {
        let Some(voice) = self.voice.clone() else {
            return Err(fdo::Error::Failed(
                "voice backend is unavailable".to_string(),
            ));
        };

        tokio::task::spawn_blocking(move || voice.listen())
            .await
            .map_err(|e| fdo::Error::Failed(format!("voice task failed: {e}")))?
            .map_err(fdo::Error::Failed)
    }

    #[zbus(signal)]
    async fn tool_executed(
        emitter: &SignalEmitter<'_>,
        name: &str,
        args: &str,
        result: &str,
    ) -> zbus::Result<()>;
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T;
}

impl<T> Pipe for T {
    fn pipe<U>(self, f: impl FnOnce(Self) -> U) -> U {
        f(self)
    }
}
