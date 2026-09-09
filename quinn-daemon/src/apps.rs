use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub aliases: Vec<String>,
    pub desktop_file: PathBuf,
}

impl AppEntry {
    pub fn generic_name(&self) -> Option<&str> {
        self.generic_name.as_deref()
    }

    pub fn desktop_file(&self) -> &Path {
        &self.desktop_file
    }
}

#[derive(Debug, Default)]
pub struct AppCatalog {
    apps: Vec<AppEntry>,
    aliases: HashMap<String, usize>,
}

impl AppCatalog {
    pub fn discover() -> Self {
        let mut catalog = Self::default();
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut dirs = Vec::with_capacity(4);
        if let Some(home) = home {
            dirs.push(home.join(".local/share/applications"));
        }
        dirs.push(PathBuf::from("/usr/local/share/applications"));
        dirs.push(PathBuf::from("/usr/share/applications"));
        dirs.push(PathBuf::from("/usr/share/gnome/applications"));

        for dir in dirs {
            catalog.scan_dir(&dir);
        }
        catalog
    }

    pub fn len(&self) -> usize {
        self.apps.len()
    }

    pub fn resolve(&self, requested: &str) -> Option<&AppEntry> {
        let normalized = normalize(requested);
        if let Some(&index) = self.aliases.get(&normalized) {
            return self.apps.get(index);
        }

        self.apps
            .iter()
            .enumerate()
            .filter_map(|(index, app)| score_match(&normalized, app).map(|score| (score, index)))
            .min_by_key(|(score, _)| *score)
            .and_then(|(_, index)| self.apps.get(index))
    }

    pub fn candidate_names(&self, requested: &str, limit: usize) -> Vec<String> {
        let normalized = normalize(requested);
        let mut scored: Vec<(u8, String)> = self
            .apps
            .iter()
            .filter_map(|app| score_match(&normalized, app).map(|score| (score, app.name.clone())))
            .collect();
        scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        scored.truncate(limit);
        scored.into_iter().map(|(_, name)| name).collect()
    }

    fn scan_dir(&mut self, dir: &Path) {
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
            if hidden_or_nodisplay(&contents) {
                continue;
            }
            let Some(name) = desktop_field(&contents, "Name") else {
                continue;
            };
            let Some(id) = path
                .file_stem()
                .and_then(|v| v.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            let generic_name = desktop_field(&contents, "GenericName");
            let mut aliases = vec![normalize(&id), normalize(&name)];
            if let Some(generic) = &generic_name {
                aliases.push(normalize(generic));
            }
            aliases.retain(|alias| !alias.is_empty());
            aliases.sort();
            aliases.dedup();

            let entry = AppEntry {
                id,
                name,
                generic_name,
                aliases: aliases.clone(),
                desktop_file: path,
            };
            let index = self.apps.len();
            self.apps.push(entry);
            for alias in aliases {
                self.aliases.entry(alias).or_insert(index);
            }
        }
    }
}

fn score_match(requested: &str, app: &AppEntry) -> Option<u8> {
    if requested.is_empty() {
        return None;
    }
    if app.aliases.iter().any(|alias| alias == requested) {
        return Some(0);
    }

    let name = normalize(&app.name);
    let id = normalize(&app.id);
    if name.starts_with(requested) || id.starts_with(requested) {
        return Some(1);
    }
    if name.contains(requested) || id.contains(requested) {
        return Some(2);
    }
    if requested.contains(&name) || requested.contains(&id) {
        return Some(3);
    }
    None
}

fn desktop_field(contents: &str, key: &str) -> Option<String> {
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

fn hidden_or_nodisplay(contents: &str) -> bool {
    contents.lines().any(|line| {
        line.eq_ignore_ascii_case("NoDisplay=true") || line.eq_ignore_ascii_case("Hidden=true")
    })
}

pub fn normalize(input: &str) -> String {
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
    use super::{normalize, score_match, AppEntry};
    use std::path::PathBuf;

    fn app(name: &str, id: &str, aliases: &[&str]) -> AppEntry {
        AppEntry {
            id: id.to_string(),
            name: name.to_string(),
            generic_name: None,
            aliases: aliases.iter().map(|v| (*v).to_string()).collect(),
            desktop_file: PathBuf::from(format!("/tmp/{id}.desktop")),
        }
    }

    #[test]
    fn normalizes_common_desktop_names() {
        assert_eq!(
            normalize("  Visual-Studio_Code.desktop "),
            "visual studio code"
        );
    }

    #[test]
    fn exact_alias_beats_substring() {
        let vscode = app(
            "Visual Studio Code",
            "code",
            &["visual studio code", "code"],
        );
        assert_eq!(score_match("code", &vscode), Some(0));
    }

    #[test]
    fn prefix_beats_contains() {
        let firefox = app("Firefox", "firefox", &["firefox"]);
        assert_eq!(score_match("fire", &firefox), Some(1));
    }
}
