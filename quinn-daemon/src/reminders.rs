use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub id: u64,
    pub text: String,
    pub when_unix: u64,
}

#[derive(Clone)]
pub struct ReminderStore {
    path: PathBuf,
    reminders: Arc<Mutex<Vec<Reminder>>>,
}

impl ReminderStore {
    pub fn load() -> Self {
        let path = reminder_path();
        let reminders = fs::read_to_string(&path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default();
        Self {
            path,
            reminders: Arc::new(Mutex::new(reminders)),
        }
    }

    pub fn create(&self, text: &str, when_unix: u64) -> Result<Reminder, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("reminder text cannot be empty".to_string());
        }
        if text.len() > 512 {
            return Err("reminder text is too long".to_string());
        }
        let now = unix_now();
        if when_unix <= now {
            return Err("reminder time must be in the future".to_string());
        }

        let mut reminders = self
            .reminders
            .lock()
            .map_err(|_| "reminder store lock is poisoned".to_string())?;
        let id = reminders
            .iter()
            .map(|r| r.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let reminder = Reminder {
            id,
            text: text.to_string(),
            when_unix,
        };
        reminders.push(reminder.clone());
        save(&self.path, &reminders)?;
        Ok(reminder)
    }

    pub fn take_due(&self) -> Vec<Reminder> {
        let now = unix_now();
        let Ok(mut reminders) = self.reminders.lock() else {
            warn!("reminder store lock is poisoned");
            return Vec::new();
        };
        let mut due = Vec::new();
        reminders.retain(|reminder| {
            if reminder.when_unix <= now {
                due.push(reminder.clone());
                false
            } else {
                true
            }
        });
        if !due.is_empty() {
            if let Err(error) = save(&self.path, &reminders) {
                warn!(%error, "failed to persist removed reminders");
            }
        }
        due
    }

    pub fn start_scheduler(&self, notify: Arc<dyn Fn(Reminder) + Send + Sync>) {
        let store = self.clone();
        tokio::spawn(async move {
            loop {
                for reminder in store.take_due() {
                    notify(reminder);
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        info!("persistent reminder scheduler ready");
    }
}

fn reminder_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let dir = PathBuf::from(home).join(".local/share/quinn");
        let _ = fs::create_dir_all(&dir);
        return dir.join("reminders.json");
    }
    PathBuf::from("reminders.json")
}

fn save(path: &PathBuf, reminders: &[Reminder]) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(reminders)
        .map_err(|e| format!("failed to encode reminders: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, data).map_err(|e| format!("failed to write reminders: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("failed to commit reminders: {e}"))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
