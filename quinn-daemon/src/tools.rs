use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub const TOOL_SCHEMAS: &str = r#"[
{"name":"open_application","description":"Open an installed desktop application by name","parameters":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}},
{"name":"open_terminal","description":"Open the user's installed terminal application","parameters":{"type":"object","properties":{},"required":[]}},
{"name":"set_volume","description":"Set the default audio output volume to an integer percentage from 0 to 100","parameters":{"type":"object","properties":{"percent":{"type":"integer","minimum":0,"maximum":100}},"required":["percent"]}},
{"name":"set_brightness","description":"Set the display brightness to an integer percentage from 0 to 100","parameters":{"type":"object","properties":{"percent":{"type":"integer","minimum":0,"maximum":100}},"required":["percent"]}}
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

pub fn execute(call: &ToolCall) -> Result<String, String> {
    match call.name.as_str() {
        "open_application" => {
            let name = call
                .arguments
                .get("name")
                .and_then(Value::as_str)
                .ok_or("missing application name")?;
            open_application(name)
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

fn open_application(requested: &str) -> Result<String, String> {
    let requested_norm = normalize(requested);
    let desktop = find_desktop_file(&requested_norm)
        .ok_or_else(|| format!("could not find installed application '{requested}'"))?;
    let desktop_id = desktop_id(&desktop)?;

    Command::new("gtk-launch")
        .arg(&desktop_id)
        .spawn()
        .map_err(|e| format!("failed to launch {requested}: {e}"))?;

    Ok(format!("opened {requested}"))
}

fn find_desktop_file(requested: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs = Vec::with_capacity(4);
    if let Some(home) = home {
        dirs.push(home.join(".local/share/applications"));
    }
    dirs.push(PathBuf::from("/usr/local/share/applications"));
    dirs.push(PathBuf::from("/usr/share/applications"));
    dirs.push(PathBuf::from("/usr/share/gnome/applications"));

    let mut best: Option<(u8, PathBuf)> = None;
    for dir in dirs {
        scan_desktop_dir(&dir, requested, &mut best);
    }
    best.map(|(_, path)| path)
}

fn scan_desktop_dir(dir: &Path, requested: &str, best: &mut Option<(u8, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|v| v.to_str()) != Some("desktop") {
            continue;
        }
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        if contents.lines().any(|line| {
            line.eq_ignore_ascii_case("NoDisplay=true") || line.eq_ignore_ascii_case("Hidden=true")
        }) {
            continue;
        }

        let stem = path
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or_default();
        let stem_norm = normalize(stem);
        let name_match = contents
            .lines()
            .filter_map(|line| line.strip_prefix("Name="))
            .map(normalize)
            .any(|name| name == requested);

        let score = if stem_norm == requested {
            0
        } else if name_match {
            1
        } else if stem_norm.replace('-', " ").contains(requested) || requested.contains(&stem_norm)
        {
            2
        } else {
            continue;
        };

        if best.as_ref().map_or(true, |(old, _)| score < *old) {
            *best = Some((score, path));
        }
    }
}

fn desktop_id(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|v| v.to_str())
        .and_then(|v| v.strip_suffix(".desktop"))
        .map(str::to_string)
        .ok_or_else(|| format!("invalid desktop entry path: {}", path.display()))
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

fn normalize(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-', '.'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn event_json(call: &ToolCall, result: &str) -> String {
    json!({
        "name": call.name,
        "arguments": call.arguments,
        "result": result,
    })
    .to_string()
}
