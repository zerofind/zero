use std::path::PathBuf;

use gpui::*;

/// An application discovered on the system.
#[derive(Clone, Debug)]
pub struct AppEntry {
    pub name: String,
    pub path: PathBuf,
}

/// Service that discovers and searches installed macOS applications.
pub struct AppService {
    apps: Vec<AppEntry>,
    loaded: bool,
}

impl AppService {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut svc = Self {
            apps: Vec::new(),
            loaded: false,
        };
        svc.load(cx);
        svc
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let apps = cx.background_executor().spawn(async { Self::scan() }).await;

            this.update(cx, |svc, _cx| {
                svc.apps = apps;
                svc.loaded = true;
            })
            .ok();
        })
        .detach();
    }

    fn scan() -> Vec<AppEntry> {
        let dirs = [
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
        ];
        let home_apps = dirs::home_dir().map(|h| h.join("Applications"));

        let mut apps = Vec::new();

        for dir in dirs.iter().chain(home_apps.iter()) {
            Self::scan_dir(dir, 0, &mut apps);
        }

        apps.sort_by_key(|a| a.name.to_lowercase());
        apps.dedup_by(|a, b| a.path == b.path);
        apps
    }

    fn scan_dir(dir: &PathBuf, depth: usize, out: &mut Vec<AppEntry>) {
        if depth > 2 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if name.ends_with(".app") {
                let display_name = name.trim_end_matches(".app").to_string();
                out.push(AppEntry {
                    name: display_name,
                    path,
                });
            } else if path.is_dir() && depth < 2 {
                Self::scan_dir(&path, depth + 1, out);
            }
        }
    }

    /// Return all discovered apps, up to `limit`.
    pub fn list(&self, limit: usize) -> Vec<AppEntry> {
        if !self.loaded {
            return Vec::new();
        }
        self.apps.iter().take(limit).cloned().collect()
    }

    /// Search apps by name. Returns up to `limit` results.
    pub fn search(&self, query: &str, limit: usize) -> Vec<AppEntry> {
        if !self.loaded || query.is_empty() {
            return Vec::new();
        }
        let query_lower = query.to_lowercase();
        let mut results: Vec<_> = self
            .apps
            .iter()
            .filter(|app| app.name.to_lowercase().contains(&query_lower))
            .cloned()
            .collect();

        // Shorter names are more relevant (exact or near matches)
        results.sort_by_key(|app| app.name.len());
        results.truncate(limit);
        results
    }

    /// Launch an application by path.
    pub fn launch(path: &PathBuf) {
        let _ = std::process::Command::new("open")
            .arg("-a")
            .arg(path)
            .spawn();
    }
}
