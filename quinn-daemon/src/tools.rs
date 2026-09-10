use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{process::Command, sync::RwLock};

use crate::{app_classifier::AppClassifier, apps::AppCatalog};

pub const TOOL_SCHEMAS: &str = r#"[
{"name":"open_application","description":"Open an installed desktop application by name or a generic type such as browser, chat, terminal, file manager, editor, media player, or game","parameters":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}},
{"name":"open_terminal","description":"Open the user's installed terminal application","parameters":{"type":"object","properties":{},"required":[]}},
{"name":"set_volume","description":"Set the default audio output volume to an integer percentage from 0 to 100","parameters":{"type":"object","properties":{"percent":{"type":"integer","minimum":0,"maximum":100}},"required":["percent"]}},
{"name":"set_brightness","description":"Set the display brightness to an integer percentage from 0 to 100","parameters":{"type":"object","properties":{"percent":{"type":"integer","minimum":0,"maximum":100}},"required":["percent"]}},
{"name":"search_files","description":"Search the user's home directory for files whose names contain a query; returns up to five deterministic matching paths","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}}
]"#;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCall {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

pub fn parse_tool_call(raw: &str) -> Result<ToolCall, String> {
    let value: Value = serde_json::from_str(raw).map_err(|e| format!("invalid tool JSON: {e}"))?;
    match value {
        Value::Object(_) => {
            serde_json::from_value(value).map_err(|e| format!("invalid tool call: {e}"))
        }
        Value::Array(values) => values
            .into_iter()
            .next()
            .ok_or_else(|| "empty tool call array".to_string())
            .and_then(|v| serde_json::from_value(v).map_err(|e| format!("invalid tool call: {e}"))),
        _ => Err("tool call must be an object or array".to_string()),
    }
}

pub fn execute(
    call: &ToolCall,
    apps: &RwLock<AppCatalog>,
    classifier: &RwLock<AppClassifier>,
) -> Result<String, String> {
    match call.name.as_str() {
        "open_application" => {
            let name = call
                .arguments
                .get("name")
                .and_then(Value::as_str)
                .ok_or("missing application name")?;
            open_application(name, apps, classifier)
        }
        "open_terminal" => open_terminal(),
        "set_volume" => {
            let percent = call
                .arguments
                .get("percent")
                .and_then(Value::as_i64)
                .ok_or("missing volume percent")?;
            set_volume(percent)
        }
        "set_brightness" => {
            let percent = call
                .arguments
                .get("percent")
                .and_then(Value::as_i64)
                .ok_or("missing brightness percent")?;
            set_brightness(percent)
        }
        "search_files" => {
            let query = call
                .arguments
                .get("query")
                .and_then(Value::as_str)
                .ok_or("missing file search query")?;
            search_files(query)
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

fn open_terminal() -> Result<String, String> {
    const CANDIDATES: &[&str] = &[
        "ptyxis",
        "kgx",
        "gnome-terminal",
        "kitty",
        "alacritty",
        "konsole",
        "x-terminal-emulator",
    ];
    for candidate in CANDIDATES {
        if Command::new(candidate).spawn().is_ok() {
            return Ok(format!("opened terminal with {candidate}"));
        }
    }
    Err("no supported terminal application was found".to_string())
}

fn open_application(
    requested: &str,
    apps: &RwLock<AppCatalog>,
    classifier: &RwLock<AppClassifier>,
) -> Result<String, String> {
    let classifier_guard = classifier
        .read()
        .map_err(|_| "application classifier lock is poisoned".to_string())?;

    if let Some(classified) = classifier_guard.resolve_type(requested) {
        let candidates = classifier_guard.type_candidates(requested, 2);
        if candidates.len() > 1 {
            return Err(format!(
                "multiple {} applications are installed: {}; please name one explicitly",
                classified.app_type.as_str(),
                candidates
                    .iter()
                    .map(|app| app.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        Command::new("gtk-launch")
            .arg(&classified.id)
            .spawn()
            .map_err(|e| format!("failed to launch {}: {e}", classified.name))?;
        return Ok(format!(
            "opened {} ({})",
            classified.name,
            classified.app_type.as_str()
        ));
    }

    if let Some(capability) = classifier_guard.resolve_capability(requested) {
        Command::new("gtk-launch")
            .arg(&capability.id)
            .spawn()
            .map_err(|e| format!("failed to launch {}: {e}", capability.name))?;
        return Ok(format!(
            "opened {} ({})",
            capability.name,
            capability.app_type.as_str()
        ));
    }
    drop(classifier_guard);

    let apps_guard = apps
        .read()
        .map_err(|_| "application catalog lock is poisoned".to_string())?;
    let app = apps_guard.resolve(requested).ok_or_else(|| {
        let candidates = apps_guard.candidate_names(requested, 3);
        if candidates.is_empty() {
            format!("could not find installed application '{requested}'")
        } else {
            format!(
                "could not confidently match '{requested}'; candidates: {}",
                candidates.join(", ")
            )
        }
    })?;

    Command::new("gtk-launch")
        .arg(&app.id)
        .spawn()
        .map_err(|e| format!("failed to launch {}: {e}", app.name))?;

    Ok(format!("opened {}", app.name))
}

fn set_volume(percent: i64) -> Result<String, String> {
    if !(0..=100).contains(&percent) {
        return Err("volume must be between 0 and 100".to_string());
    }

    let value = format!("{percent}%");
    if Command::new("wpctl")
        .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &value])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(format!("volume set to {percent}%"));
    }

    if Command::new("pactl")
        .args(["set-sink-volume", "@DEFAULT_SINK@", &value])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(format!("volume set to {percent}%"));
    }

    Err("neither wpctl nor pactl could set the default output volume".to_string())
}

fn set_brightness(percent: i64) -> Result<String, String> {
    if !(0..=100).contains(&percent) {
        return Err("brightness must be between 0 and 100".to_string());
    }

    let value = format!("{percent}%");
    if Command::new("brightnessctl")
        .args(["set", &value])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(format!("brightness set to {percent}%"));
    }

    Err("brightnessctl is not available or could not set the display brightness".to_string())
}

fn search_files(query: &str) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("file search query cannot be empty".to_string());
    }
    if query.len() > 128 {
        return Err("file search query is too long".to_string());
    }

    let home = std::env::var_os("HOME").ok_or_else(|| "HOME is not set".to_string())?;

    let pattern = format!("*{query}*");
    let output = Command::new("find")
        .arg(&home)
        .args(["-maxdepth", "6", "-type", "f", "-iname"])
        .arg(&pattern)
        .arg("-print")
        .output()
        .map_err(|e| format!("failed to search files: {e}"))?;

    if !output.status.success() && output.stdout.is_empty() {
        return Err("file search failed".to_string());
    }

    let mut matches: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect();
    matches.sort_unstable();
    matches.truncate(5);
    if matches.is_empty() {
        return Ok(format!("no files matching '{query}' were found"));
    }

    Ok(format!("found files: {}", matches.join("; ")))
}

pub fn event_json(call: &ToolCall, result: &str) -> String {
    json!({
        "name": call.name,
        "arguments": call.arguments,
        "result": result,
    })
    .to_string()
}
