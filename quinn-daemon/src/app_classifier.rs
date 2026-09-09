use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppType {
    Browser,
    Chat,
    Editor,
    Terminal,
    FileManager,
    MediaPlayer,
    ImageViewer,
    Game,
    Development,
    Office,
    Unknown,
}

impl AppType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::Chat => "chat",
            Self::Editor => "editor",
            Self::Terminal => "terminal",
            Self::FileManager => "file-manager",
            Self::MediaPlayer => "media-player",
            Self::ImageViewer => "image-viewer",
            Self::Game => "game",
            Self::Development => "development",
            Self::Office => "office",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClassifiedApp {
    pub id: String,
    pub name: String,
    pub app_type: AppType,
    pub capabilities: Vec<String>,
    pub desktop_file: PathBuf,
}

#[derive(Debug, Default, Clone)]
pub struct AppClassifier {
    apps: Vec<ClassifiedApp>,
}

impl AppClassifier {
    pub fn discover() -> Self {
        let mut classifier = Self::default();
        for dir in application_dirs() {
            classifier.scan_dir(&dir);
        }
        classifier
    }

    pub fn len(&self) -> usize {
        self.apps.len()
    }

    pub fn resolve_type(&self, requested: &str) -> Option<&ClassifiedApp> {
        let requested = normalize(requested);
        let kind = match requested.as_str() {
            "browser" | "web browser" | "webbrowser" => AppType::Browser,
            "chat" | "messenger" | "messaging" => AppType::Chat,
            "editor" | "text editor" => AppType::Editor,
            "terminal" | "terminal emulator" => AppType::Terminal,
            "file manager" | "filemanager" => AppType::FileManager,
            "media player" | "media" => AppType::MediaPlayer,
            "image viewer" | "photo viewer" => AppType::ImageViewer,
            "game" => AppType::Game,
            "development" | "developer tools" | "ide" | "code editor" => AppType::Development,
            "office" => AppType::Office,
            _ => return None,
        };
        self.apps.iter().find(|app| app.app_type == kind)
    }

    pub fn candidates(&self, requested: &str, limit: usize) -> Vec<&ClassifiedApp> {
        let requested = normalize(requested);
        let mut matches: Vec<_> = self
            .apps
            .iter()
            .filter(|app| normalize(&app.name).contains(&requested))
            .collect();
        matches.sort_by(|a, b| a.name.cmp(&b.name));
        matches.truncate(limit);
        matches
    }

    fn scan_dir(&mut self, dir: &PathBuf) {
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
            if hidden_or_nodisplay(&contents) || field(&contents, "Type").as_deref() != Some("Application") {
                continue;
            }
            let Some(name) = field(&contents, "Name") else {
                continue;
            };
            let Some(id) = path.file_stem().and_then(|v| v.to_str()).map(str::to_string) else {
                continue;
            };
            let generic = field(&contents, "GenericName");
            let categories = list_field(&contents, "Categories");
            let keywords = list_field(&contents, "Keywords");
            let mime_types = list_field(&contents, "MimeType");
            let app_type = classify(&name, generic.as_deref(), &categories, &keywords, &mime_types);
            let capabilities = capabilities(&categories, &keywords, &mime_types);

            self.apps.push(ClassifiedApp {
                id,
                name,
                app_type,
                capabilities,
                desktop_file: path,
            });
        }
    }
}

fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::with_capacity(4);
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    dirs.push(PathBuf::from("/usr/local/share/applications"));
    dirs.push(PathBuf::from("/usr/share/applications"));
    dirs.push(PathBuf::from("/usr/share/gnome/applications"));
    dirs
}

fn classify(name: &str, generic: Option<&str>, categories: &[String], keywords: &[String], mime_types: &[String]) -> AppType {
    let haystack = normalize(&format!(
        "{} {} {} {}",
        name,
        generic.unwrap_or_default(),
        categories.join(" "),
        keywords.join(" ")
    ));
    let has = |s: &str| haystack.contains(s);
    let category = |s: &str| categories.iter().any(|v| normalize(v) == s);
    let mime = |s: &str| mime_types.iter().any(|v| v.eq_ignore_ascii_case(s));

    if category("webbrowser")
        || mime("x-scheme-handler/http")
        || mime("x-scheme-handler/https")
        || mime("text/html")
        || has("web browser")
        || has("browser")
    {
        return AppType::Browser;
    }
    if category("chat") || category("instantmessaging") || has("messenger") || has("chat") {
        return AppType::Chat;
    }
    if category("development") || category("ide") || has("code editor") || has("developer") {
        return AppType::Development;
    }
    if category("texteditor") || has("text editor") || has("editor") {
        return AppType::Editor;
    }
    if category("filemanager") || has("file manager") || mime("inode/directory") {
        return AppType::FileManager;
    }
    if category("audiovideo") || has("media player") || has("media") {
        return AppType::MediaPlayer;
    }
    if category("graphics") || has("image viewer") || has("photo viewer") {
        return AppType::ImageViewer;
    }
    if category("game") || has("game") {
        return AppType::Game;
    }
    if category("office") || has("office") || has("spreadsheet") || has("word processor") {
        return AppType::Office;
    }
    if category("terminalemulator") || has("terminal") {
        return AppType::Terminal;
    }
    AppType::Unknown
}

fn capabilities(categories: &[String], keywords: &[String], mime_types: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let add = |out: &mut Vec<String>, value: &str| {
        if !out.iter().any(|v| v == value) {
            out.push(value.to_string());
        }
    };
    if mime_types.iter().any(|v| v.eq_ignore_ascii_case("text/html")) {
        add(&mut out, "html");
    }
    if mime_types.iter().any(|v| v.eq_ignore_ascii_case("x-scheme-handler/http"))
        || mime_types.iter().any(|v| v.eq_ignore_ascii_case("x-scheme-handler/https"))
    {
        add(&mut out, "web");
        add(&mut out, "http");
        add(&mut out, "https");
    }
    if mime_types.iter().any(|v| v.eq_ignore_ascii_case("inode/directory")) {
        add(&mut out, "directories");
    }
    if mime_types.iter().any(|v| v.starts_with("audio/")) {
        add(&mut out, "audio");
    }
    if mime_types.iter().any(|v| v.starts_with("video/")) {
        add(&mut out, "video");
    }
    if mime_types.iter().any(|v| v.starts_with("image/")) {
        add(&mut out, "images");
    }
    if categories.iter().any(|v| normalize(v) == "webbrowser") || keywords.iter().any(|v| normalize(v).contains("browser")) {
        add(&mut out, "browser");
    }
    out
}

fn field(contents: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    contents
        .lines()
        .find_map(|line| line.strip_prefix(&prefix).map(str::trim).filter(|v| !v.is_empty()))
        .map(str::to_string)
}

fn list_field(contents: &str, key: &str) -> Vec<String> {
    field(contents, key)
        .unwrap_or_default()
        .split(';')
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .collect()
}

fn hidden_or_nodisplay(contents: &str) -> bool {
    contents.lines().any(|line| {
        line.eq_ignore_ascii_case("NoDisplay=true") || line.eq_ignore_ascii_case("Hidden=true")
    })
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

#[cfg(test)]
mod tests {
    use super::{classify, AppType};

    #[test]
    fn helium_like_metadata_is_browser() {
        let result = classify(
            "Helium",
            Some("Web Browser"),
            &["Network".into(), "WebBrowser".into()],
            &[],
            &["text/html".into(), "x-scheme-handler/https".into()],
        );
        assert_eq!(result, AppType::Browser);
    }
}
