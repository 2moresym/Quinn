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

impl ClassifiedApp {
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    pub fn desktop_file(&self) -> &std::path::Path {
        &self.desktop_file
    }
}

#[derive(Debug, Default, Clone)]
pub struct AppClassifier {
    apps: Vec<ClassifiedApp>,
}

impl AppClassifier {
    pub fn discover() -> Self {
        let mut classifier = Self::default();
        classifier.refresh();
        classifier
    }

    pub fn refresh(&mut self) {
        let mut fresh = Self::default();
        for dir in application_dirs() {
            fresh.scan_dir(&dir);
        }
        *self = fresh;
    }

    pub fn len(&self) -> usize {
        self.apps.len()
    }

    pub fn resolve_type(&self, requested: &str) -> Option<&ClassifiedApp> {
        self.type_candidates(requested, 1).into_iter().next()
    }

    pub fn type_candidates(&self, requested: &str, limit: usize) -> Vec<&ClassifiedApp> {
        let requested = normalize(requested);
        let Some(kind) = parse_type(&requested) else {
            return Vec::new();
        };

        let mut candidates: Vec<_> = self
            .apps
            .iter()
            .filter(|app| app.app_type == kind)
            .collect();
        candidates.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        candidates.truncate(limit);
        candidates
    }

    pub fn resolve_capability(&self, requested: &str) -> Option<&ClassifiedApp> {
        let requested = normalize(requested);
        if requested.is_empty() {
            return None;
        }

        self.apps
            .iter()
            .enumerate()
            .filter_map(|(index, app)| {
                capability_score(&requested, app).map(|score| (score, index))
            })
            .min_by(|(score_a, index_a), (score_b, index_b)| {
                score_a
                    .cmp(score_b)
                    .then_with(|| self.apps[*index_a].name.cmp(&self.apps[*index_b].name))
                    .then_with(|| self.apps[*index_a].id.cmp(&self.apps[*index_b].id))
            })
            .map(|(_, index)| &self.apps[index])
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
            if hidden_or_nodisplay(&contents)
                || field(&contents, "Type").as_deref() != Some("Application")
            {
                continue;
            }
            let Some(name) = field(&contents, "Name") else {
                continue;
            };
            let Some(id) = path
                .file_stem()
                .and_then(|v| v.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            let generic = field(&contents, "GenericName");
            let categories = list_field(&contents, "Categories");
            let keywords = list_field(&contents, "Keywords");
            let mime_types = list_field(&contents, "MimeType");
            let app_type = classify(
                &name,
                generic.as_deref(),
                &categories,
                &keywords,
                &mime_types,
            );
            let capabilities = capabilities(&categories, &mime_types);

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

fn classify(
    name: &str,
    generic: Option<&str>,
    categories: &[String],
    keywords: &[String],
    mime_types: &[String],
) -> AppType {
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
    if category("audiovideo") || has("media player") {
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

fn capabilities(categories: &[String], mime_types: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let add = |out: &mut Vec<String>, value: &str| {
        if !out.iter().any(|v| v == value) {
            out.push(value.to_string());
        }
    };
    if mime_types
        .iter()
        .any(|v| v.eq_ignore_ascii_case("text/html"))
    {
        add(&mut out, "html");
    }
    if mime_types
        .iter()
        .any(|v| v.eq_ignore_ascii_case("x-scheme-handler/http"))
        || mime_types
            .iter()
            .any(|v| v.eq_ignore_ascii_case("x-scheme-handler/https"))
    {
        add(&mut out, "web");
        add(&mut out, "http");
        add(&mut out, "https");
    }
    if mime_types
        .iter()
        .any(|v| v.eq_ignore_ascii_case("inode/directory"))
    {
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
    if categories.iter().any(|v| normalize(v) == "webbrowser") {
        add(&mut out, "browser");
    }
    out
}

fn capability_score(requested: &str, app: &ClassifiedApp) -> Option<u8> {
    let requested = match requested {
        "web browser" | "webbrowser" => "browser",
        "file manager" | "filemanager" => "file-manager",
        "media" => "media-player",
        "photo viewer" => "image-viewer",
        "terminal emulator" => "terminal",
        "developer tools" | "ide" | "code editor" => "development",
        value => value,
    };

    if app.app_type.as_str() == requested {
        return Some(0);
    }
    if app.capabilities.iter().any(|v| v == requested) {
        return Some(1);
    }
    if app.capabilities.iter().any(|v| v.contains(requested)) {
        return Some(2);
    }
    None
}

fn parse_type(input: &str) -> Option<AppType> {
    Some(match input {
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
    })
}

fn field(contents: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    contents
        .lines()
        .find_map(|line| {
            line.strip_prefix(&prefix)
                .map(str::trim)
                .filter(|v| !v.is_empty())
        })
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
    use super::{capability_score, classify, AppType, ClassifiedApp};
    use std::path::PathBuf;

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

    #[test]
    fn browser_capability_resolves_to_browser_type() {
        let app = ClassifiedApp {
            id: "helium".into(),
            name: "Helium".into(),
            app_type: AppType::Browser,
            capabilities: vec!["web".into(), "https".into()],
            desktop_file: PathBuf::from("/tmp/helium.desktop"),
        };
        assert_eq!(capability_score("browser", &app), Some(0));
        assert_eq!(capability_score("web", &app), Some(1));
    }
}
