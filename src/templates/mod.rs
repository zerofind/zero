//! Backup templates for common use cases
//!
//! Templates define WHAT to backup (source paths + excludes).
//! Automations define WHEN + WHERE (trigger + destination + template).
//!
//! ## Built-in Templates
//!
//! - **Mac Essentials**: Documents, Downloads, Desktop, Pictures, Movies, Music
//! - **macOS Developer**: Essentials + code, dotfiles, SSH keys, configs

use std::path::{Path, PathBuf};

/// A backup template defining what to backup
#[derive(Debug, Clone)]
pub struct Template {
    /// Unique identifier (e.g., "mac-essentials")
    pub id: &'static str,
    /// Human-readable name (e.g., "Mac Essentials")
    pub name: &'static str,
    /// Description of what this template backs up
    pub description: &'static str,
    /// Source paths to include (relative to home directory)
    pub sources: &'static [TemplateSource],
    /// Patterns to exclude (applied to all sources)
    pub excludes: &'static [&'static str],
}

/// A source path in a template
#[derive(Debug, Clone)]
pub struct TemplateSource {
    /// Path relative to home directory (e.g., "Documents")
    pub path: &'static str,
    /// If true, skip this path if it doesn't exist (no error)
    pub optional: bool,
    /// Human-readable description
    pub description: &'static str,
}

impl TemplateSource {
    /// Create a required source
    const fn required(path: &'static str, description: &'static str) -> Self {
        Self {
            path,
            optional: false,
            description,
        }
    }

    /// Create an optional source (skipped if doesn't exist)
    const fn optional(path: &'static str, description: &'static str) -> Self {
        Self {
            path,
            optional: true,
            description,
        }
    }
}

/// Cloud sync folders that should be auto-excluded
/// (they have their own sync, backing them up causes duplicates)
const CLOUD_FOLDERS: &[&str] = &[
    "Dropbox",
    "Box",
    "Google Drive",
    "OneDrive",
    "iCloud Drive (Archive)",
    "Library/Mobile Documents", // iCloud Drive actual location
    "Library/CloudStorage",     // macOS cloud storage mount point
];

/// System folders that should generally be excluded
const SYSTEM_EXCLUDES: &[&str] = &[
    ".Trash",
    ".DS_Store",
    "._*",
    ".localized",
    ".CFUserTextEncoding",
];

// =============================================================================
// Built-in Templates (static data)
// =============================================================================

/// Mac Essentials sources
const MAC_ESSENTIALS_SOURCES: &[TemplateSource] = &[
    TemplateSource::required("Documents", "Documents and files"),
    TemplateSource::required("Downloads", "Downloaded files"),
    TemplateSource::required("Desktop", "Desktop files"),
    TemplateSource::required("Pictures", "Photos and images"),
    TemplateSource::optional("Movies", "Videos and movies"),
    TemplateSource::optional("Music", "Music library"),
];

/// macOS Developer sources
const MACOS_DEVELOPER_SOURCES: &[TemplateSource] = &[
    // Standard folders
    TemplateSource::required("Documents", "Documents and files"),
    TemplateSource::required("Downloads", "Downloaded files"),
    TemplateSource::required("Desktop", "Desktop files"),
    TemplateSource::required("Pictures", "Photos and images"),
    TemplateSource::optional("Movies", "Videos and movies"),
    TemplateSource::optional("Music", "Music library"),
    // Developer folders
    TemplateSource::optional("code", "Code projects (~/code)"),
    TemplateSource::optional("Code", "Code projects (~/Code)"),
    TemplateSource::optional("projects", "Projects (~/projects)"),
    TemplateSource::optional("Projects", "Projects (~/Projects)"),
    TemplateSource::optional("dev", "Dev projects (~/dev)"),
    TemplateSource::optional("src", "Source code (~/src)"),
    TemplateSource::optional("repos", "Git repos (~/repos)"),
    TemplateSource::optional("github", "GitHub repos (~/github)"),
    TemplateSource::optional("work", "Work projects (~/work)"),
    // Dotfiles and configs
    TemplateSource::optional(".ssh", "SSH keys and config"),
    TemplateSource::optional(".gnupg", "GPG keys"),
    TemplateSource::optional(".config", "App configurations"),
    TemplateSource::optional(".zshrc", "Zsh config"),
    TemplateSource::optional(".bashrc", "Bash config"),
    TemplateSource::optional(".bash_profile", "Bash profile"),
    TemplateSource::optional(".gitconfig", "Git config"),
    TemplateSource::optional(".gitignore_global", "Global gitignore"),
    TemplateSource::optional(".vimrc", "Vim config"),
    TemplateSource::optional(".tmux.conf", "Tmux config"),
    TemplateSource::optional(".zprofile", "Zsh profile"),
    TemplateSource::optional(".profile", "Shell profile"),
    TemplateSource::optional(".netrc", "Network credentials"),
    TemplateSource::optional(".npmrc", "NPM config"),
    TemplateSource::optional(".cargo/config.toml", "Cargo config"),
    TemplateSource::optional(".aws", "AWS credentials and config"),
];

/// macOS Developer excludes
const MACOS_DEVELOPER_EXCLUDES: &[&str] = &[
    // Build artifacts and dependencies (large, reproducible)
    "node_modules",
    "target",      // Rust
    "build",       // Generic build output
    "dist",        // Distribution builds
    ".next",       // Next.js
    ".nuxt",       // Nuxt.js
    "__pycache__", // Python
    "*.pyc",
    ".venv", // Python virtualenv
    "venv",
    ".tox",
    ".eggs",
    "*.egg-info",
    ".gradle",     // Gradle
    ".maven",      // Maven
    "vendor",      // Go, PHP, Ruby
    "Pods",        // CocoaPods
    ".build",      // Swift
    "DerivedData", // Xcode
    // Version control (already tracked, large)
    ".git",
    ".svn",
    ".hg",
    // IDE and editor files (reproducible)
    ".idea",
    ".vscode",
    "*.swp",
    "*.swo",
    "*~",
    // OS files
    ".DS_Store",
    "._*",
    "Thumbs.db",
    // Logs and temp files
    "*.log",
    "*.tmp",
    ".cache",
    ".npm",
    ".yarn",
];

// =============================================================================
// Template Registry
// =============================================================================

/// Mac Essentials - the standard user folders
pub const MAC_ESSENTIALS: Template = Template {
    id: "mac-essentials",
    name: "Mac Essentials",
    description: "Documents, Downloads, Desktop, Pictures, Movies, Music - everything a typical user needs backed up",
    sources: MAC_ESSENTIALS_SOURCES,
    excludes: &[],
};

/// macOS Developer - essentials plus dev tools and configs
pub const MACOS_DEVELOPER: Template = Template {
    id: "macos-developer",
    name: "macOS Developer",
    description: "Mac Essentials + code projects, SSH keys, shell configs, and dev tool settings",
    sources: MACOS_DEVELOPER_SOURCES,
    excludes: MACOS_DEVELOPER_EXCLUDES,
};

/// All available templates
pub const ALL_TEMPLATES: &[Template] = &[MAC_ESSENTIALS, MACOS_DEVELOPER];

/// Get all available templates
pub fn all_templates() -> &'static [Template] {
    ALL_TEMPLATES
}

/// Get a template by ID
pub fn get_template(id: &str) -> Option<&'static Template> {
    ALL_TEMPLATES.iter().find(|t| t.id == id)
}

/// List template IDs
pub fn template_ids() -> Vec<&'static str> {
    ALL_TEMPLATES.iter().map(|t| t.id).collect()
}

// =============================================================================
// Template Resolution
// =============================================================================

/// A resolved template with absolute paths and detected exclusions
#[derive(Debug, Clone)]
pub struct ResolvedTemplate {
    /// Original template
    pub template: Template,
    /// Resolved source paths (absolute, existing only)
    pub sources: Vec<ResolvedSource>,
    /// All exclude patterns (template + cloud + system)
    pub excludes: Vec<String>,
    /// Cloud folders that were detected and auto-excluded
    pub detected_cloud_folders: Vec<PathBuf>,
}

/// A resolved source path
#[derive(Debug, Clone)]
pub struct ResolvedSource {
    /// Absolute path
    pub path: PathBuf,
    /// Original template source
    pub source: TemplateSource,
    /// Whether this path exists
    pub exists: bool,
}

impl Template {
    /// Resolve this template to absolute paths, detecting cloud folders
    pub fn resolve(&self) -> ResolvedTemplate {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));

        // Resolve source paths
        let sources: Vec<ResolvedSource> = self
            .sources
            .iter()
            .map(|src| {
                let path = home.join(src.path);
                let exists = path.exists();
                ResolvedSource {
                    path,
                    source: (*src).clone(),
                    exists,
                }
            })
            .collect();

        // Detect cloud folders
        let detected_cloud_folders: Vec<PathBuf> = CLOUD_FOLDERS
            .iter()
            .map(|p| home.join(p))
            .filter(|p| p.exists())
            .collect();

        // Build exclude list
        let mut excludes: Vec<String> = Vec::new();

        // Add template excludes
        excludes.extend(self.excludes.iter().map(|&s| s.to_string()));

        // Add system excludes
        excludes.extend(SYSTEM_EXCLUDES.iter().map(|s| s.to_string()));

        // Add detected cloud folders (by name, not full path)
        for cloud_path in &detected_cloud_folders {
            if let Some(name) = cloud_path.file_name() {
                excludes.push(name.to_string_lossy().to_string());
            }
        }

        ResolvedTemplate {
            template: self.clone(),
            sources,
            excludes,
            detected_cloud_folders,
        }
    }

    /// Get only the source paths that exist
    pub fn existing_sources(&self) -> Vec<PathBuf> {
        self.resolve()
            .sources
            .into_iter()
            .filter(|s| s.exists)
            .map(|s| s.path)
            .collect()
    }

    /// Get sources as a Vec (for compatibility)
    pub fn sources_vec(&self) -> Vec<TemplateSource> {
        self.sources.to_vec()
    }

    /// Get excludes as a Vec (for compatibility)
    pub fn excludes_vec(&self) -> Vec<&'static str> {
        self.excludes.to_vec()
    }
}

impl ResolvedTemplate {
    /// Get only sources that exist
    pub fn existing_sources(&self) -> Vec<&ResolvedSource> {
        self.sources.iter().filter(|s| s.exists).collect()
    }

    /// Get sources that are missing (required but don't exist)
    pub fn missing_required_sources(&self) -> Vec<&ResolvedSource> {
        self.sources
            .iter()
            .filter(|s| !s.exists && !s.source.optional)
            .collect()
    }

    /// Check if template is valid (all required sources exist)
    pub fn is_valid(&self) -> bool {
        self.missing_required_sources().is_empty()
    }

    /// Total estimated size of backup (quick estimate based on existing paths)
    pub fn estimate_size(&self) -> std::io::Result<u64> {
        let mut total = 0u64;
        for source in self.existing_sources() {
            total += dir_size_estimate(&source.path)?;
        }
        Ok(total)
    }
}

/// Quick directory size estimate (doesn't recurse deeply, just top level)
fn dir_size_estimate(path: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;

    if path.is_file() {
        return Ok(path.metadata()?.len());
    }

    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                total += metadata.len();
            } else if metadata.is_dir() {
                // For directories, just add a rough estimate or recurse one level
                // To keep it fast, we'll just count the dir as 4KB (one block)
                total += 4096;
            }
        }
    }

    Ok(total)
}

// =============================================================================
// Display
// =============================================================================

impl std::fmt::Display for Template {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} ({})", self.name, self.id)?;
        writeln!(f, "  {}", self.description)?;
        writeln!(f)?;
        writeln!(f, "  Sources:")?;
        for src in self.sources {
            let marker = if src.optional { "○" } else { "●" };
            writeln!(f, "    {} ~/{} - {}", marker, src.path, src.description)?;
        }
        if !self.excludes.is_empty() {
            writeln!(f)?;
            let excludes_str: Vec<&str> = self.excludes.to_vec();
            writeln!(f, "  Excludes: {}", excludes_str.join(", "))?;
        }
        Ok(())
    }
}

impl std::fmt::Display for ResolvedTemplate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} ({})", self.template.name, self.template.id)?;
        writeln!(f, "  {}", self.template.description)?;
        writeln!(f)?;
        writeln!(f, "  Sources:")?;
        for src in &self.sources {
            let status = if src.exists {
                "✓"
            } else if src.source.optional {
                "○"
            } else {
                "✗"
            };
            writeln!(
                f,
                "    {} ~/{} - {}",
                status, src.source.path, src.source.description
            )?;
        }
        if !self.detected_cloud_folders.is_empty() {
            writeln!(f)?;
            writeln!(f, "  Auto-excluded cloud folders:")?;
            for cloud in &self.detected_cloud_folders {
                writeln!(f, "    • {}", cloud.display())?;
            }
        }
        if !self.excludes.is_empty() {
            writeln!(f)?;
            writeln!(f, "  Excludes: {}", self.excludes.join(", "))?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod mod_test;
