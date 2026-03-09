//! Code structure indexing
//!
//! Extracts code structure (functions, types, traits) from Rust and Go projects,
//! stores compressed indexes in `~/.zero/code/`, and provides search/overview APIs
//! for MCP tools and CLI.

pub mod element;
pub mod parsers;
pub mod persistence;
pub mod scanner;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub use element::{CodeElement, ElementKind, Language, ProjectSummary, Visibility};
pub use scanner::ScanError;

/// Metadata about an indexed project (stored in registry.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeProject {
    pub path: PathBuf,
    pub hash: String,
    pub languages: Vec<Language>,
    pub symbol_count: usize,
    pub file_count: usize,
    pub lines_of_code: usize,
    pub last_indexed: u64,
}

/// Search options for code_search
pub struct CodeSearchOpts {
    pub kind: Option<ElementKind>,
    pub language: Option<Language>,
    pub project: Option<PathBuf>,
    pub limit: usize,
}

impl Default for CodeSearchOpts {
    fn default() -> Self {
        Self {
            kind: None,
            language: None,
            project: None,
            limit: 30,
        }
    }
}

/// A search result with project context
pub struct CodeSearchResult {
    pub element: CodeElement,
    pub project_path: String,
}

/// Overview of a project's structure
pub struct ProjectOverview {
    pub path: String,
    pub languages: Vec<Language>,
    pub file_count: usize,
    pub lines_of_code: usize,
    pub symbol_count: usize,
    /// Module path -> (description hint, key types)
    pub modules: Vec<(String, Vec<String>)>,
    pub key_types: Vec<String>,
}

/// Manages code indexes across projects
pub struct CodeIndex {
    index_dir: PathBuf,
    projects: HashMap<String, CodeProject>,
    cache: HashMap<String, ProjectSummary>,
}

impl CodeIndex {
    /// Create a new CodeIndex using the default directory (~/.zero/code/)
    pub fn new() -> Result<Self, CodeIndexError> {
        let index_dir = crate::dirs::code_dir().ok_or_else(|| {
            CodeIndexError::Config("Cannot determine ~/.zero/code/ directory".into())
        })?;

        std::fs::create_dir_all(&index_dir)?;

        let mut ci = Self {
            index_dir,
            projects: HashMap::new(),
            cache: HashMap::new(),
        };
        ci.load_registry()?;
        Ok(ci)
    }

    /// Create with a custom directory (for testing)
    pub fn with_dir(index_dir: PathBuf) -> Result<Self, CodeIndexError> {
        std::fs::create_dir_all(&index_dir)?;

        let mut ci = Self {
            index_dir,
            projects: HashMap::new(),
            cache: HashMap::new(),
        };
        ci.load_registry()?;
        Ok(ci)
    }

    // ========================================================================
    // Registry
    // ========================================================================

    fn registry_path(&self) -> PathBuf {
        self.index_dir.join("registry.json")
    }

    fn load_registry(&mut self) -> Result<(), CodeIndexError> {
        let path = self.registry_path();
        if !path.exists() {
            return Ok(());
        }
        let data = std::fs::read_to_string(&path)?;
        let projects: Vec<CodeProject> =
            serde_json::from_str(&data).map_err(|e| CodeIndexError::Config(e.to_string()))?;

        for p in projects {
            self.projects.insert(p.hash.clone(), p);
        }
        Ok(())
    }

    fn save_registry(&self) -> Result<(), CodeIndexError> {
        let projects: Vec<&CodeProject> = self.projects.values().collect();
        let data = serde_json::to_string_pretty(&projects)
            .map_err(|e| CodeIndexError::Config(e.to_string()))?;
        let tmp = self.registry_path().with_extension("json.tmp");
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, self.registry_path())?;
        Ok(())
    }

    // ========================================================================
    // Indexing
    // ========================================================================

    /// Index a single project
    pub fn index_project(&mut self, path: &Path) -> Result<&CodeProject, CodeIndexError> {
        let canonical = path.canonicalize().map_err(CodeIndexError::Io)?;

        let hash = hash_path(&canonical);

        // Scan
        let summary =
            scanner::scan_project(&canonical).map_err(|e| CodeIndexError::Scan(e.to_string()))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let project = CodeProject {
            path: canonical,
            hash: hash.clone(),
            languages: summary.languages(),
            symbol_count: summary.elements.len(),
            file_count: summary.files_processed,
            lines_of_code: summary.lines_of_code,
            last_indexed: now,
        };

        // Persist .cidx
        let cidx_path = self.index_dir.join(format!("{}.cidx", hash));
        persistence::save_summary(&summary, &cidx_path)
            .map_err(|e| CodeIndexError::Persist(e.to_string()))?;

        // Update registry
        self.cache.insert(hash.clone(), summary);
        self.projects.insert(hash.clone(), project);
        self.save_registry()?;

        Ok(self.projects.get(&hash).expect("just inserted"))
    }

    /// Remove a project from the index
    pub fn remove_project(&mut self, path: &Path) -> Result<(), CodeIndexError> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let hash = hash_path(&canonical);

        self.projects.remove(&hash);
        self.cache.remove(&hash);

        let cidx_path = self.index_dir.join(format!("{}.cidx", hash));
        if cidx_path.exists() {
            std::fs::remove_file(&cidx_path)?;
        }

        self.save_registry()?;
        Ok(())
    }

    /// List all indexed projects
    #[must_use]
    pub fn indexed_projects(&self) -> Vec<&CodeProject> {
        let mut projects: Vec<&CodeProject> = self.projects.values().collect();
        projects.sort_by(|a, b| a.path.cmp(&b.path));
        projects
    }

    /// Discover and index all projects under a root directory
    pub fn index_all(
        &mut self,
        root: &Path,
        git_only: bool,
    ) -> Result<Vec<String>, CodeIndexError> {
        let discovered = scanner::discover_projects(root, git_only);
        let mut indexed = Vec::new();

        for project_path in discovered {
            match self.index_project(&project_path) {
                Ok(p) => indexed.push(p.path.display().to_string()),
                Err(e) => {
                    tracing::warn!(path = %project_path.display(), error = %e, "Failed to index project");
                }
            }
        }

        Ok(indexed)
    }

    // ========================================================================
    // Queries
    // ========================================================================

    /// Search for symbols across all indexed projects
    pub fn search(
        &mut self,
        query: &str,
        opts: &CodeSearchOpts,
    ) -> Result<Vec<CodeSearchResult>, CodeIndexError> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        let hashes: Vec<(String, String)> = if let Some(project) = &opts.project {
            let canonical = project.canonicalize().unwrap_or_else(|_| project.clone());
            let hash = hash_path(&canonical);
            if let Some(p) = self.projects.get(&hash) {
                vec![(hash, p.path.display().to_string())]
            } else {
                return Ok(Vec::new());
            }
        } else {
            self.projects
                .iter()
                .map(|(h, p)| (h.clone(), p.path.display().to_string()))
                .collect()
        };

        for (hash, project_path) in &hashes {
            let summary = self.load_summary(hash)?;

            for element in &summary.elements {
                // Kind filter
                if let Some(kind) = opts.kind
                    && element.kind != kind
                {
                    continue;
                }
                // Language filter
                if let Some(lang) = opts.language
                    && element.language != lang
                {
                    continue;
                }
                // Name match
                if element.name.to_lowercase().contains(&query_lower)
                    || element.signature.to_lowercase().contains(&query_lower)
                {
                    results.push(CodeSearchResult {
                        element: element.clone(),
                        project_path: project_path.clone(),
                    });
                }

                if results.len() >= opts.limit {
                    return Ok(results);
                }
            }
        }

        // Sort: exact name match first, then by name length (shorter = more relevant)
        results.sort_by(|a, b| {
            let a_exact = a.element.name.to_lowercase() == query_lower;
            let b_exact = b.element.name.to_lowercase() == query_lower;
            b_exact
                .cmp(&a_exact)
                .then(a.element.name.len().cmp(&b.element.name.len()))
        });

        results.truncate(opts.limit);
        Ok(results)
    }

    /// List all symbols in a project (no query needed)
    pub fn project_symbols(
        &mut self,
        project: &Path,
        opts: &CodeSearchOpts,
    ) -> Result<Vec<CodeSearchResult>, CodeIndexError> {
        let canonical = project
            .canonicalize()
            .unwrap_or_else(|_| project.to_path_buf());
        let hash = hash_path(&canonical);

        let project_path = self
            .projects
            .get(&hash)
            .map(|p| p.path.display().to_string())
            .unwrap_or_else(|| canonical.display().to_string());

        let summary = self.load_summary(&hash)?;
        let mut results = Vec::new();

        for element in summary.public_only() {
            if let Some(kind) = opts.kind
                && element.kind != kind
            {
                continue;
            }
            if let Some(lang) = opts.language
                && element.language != lang
            {
                continue;
            }
            results.push(CodeSearchResult {
                element: element.clone(),
                project_path: project_path.clone(),
            });
            if results.len() >= opts.limit {
                break;
            }
        }

        Ok(results)
    }

    /// Get an overview of a project
    pub fn overview(&mut self, project: &Path) -> Result<Option<ProjectOverview>, CodeIndexError> {
        let canonical = project
            .canonicalize()
            .unwrap_or_else(|_| project.to_path_buf());
        let hash = hash_path(&canonical);

        let proj = match self.projects.get(&hash) {
            Some(p) => p.clone(),
            None => return Ok(None),
        };

        let summary = self.load_summary(&hash)?;

        // Build module structure from file paths
        let by_file = summary.by_file();
        let mut module_map: HashMap<String, Vec<String>> = HashMap::new();

        for (file_path, elements) in &by_file {
            // Extract module prefix (e.g., "src/index/" from "src/index/search.rs")
            let module = Path::new(file_path)
                .parent()
                .map(|p| {
                    let s = p.display().to_string();
                    if s.is_empty() {
                        ".".to_string()
                    } else {
                        format!("{}/", s)
                    }
                })
                .unwrap_or_else(|| ".".to_string());

            let entry = module_map.entry(module).or_default();
            for elem in elements {
                if elem.visibility.is_public()
                    && matches!(
                        elem.kind,
                        ElementKind::Struct
                            | ElementKind::Enum
                            | ElementKind::Trait
                            | ElementKind::Interface
                    )
                    && !entry.contains(&elem.name)
                {
                    entry.push(elem.name.clone());
                }
            }
        }

        let mut modules: Vec<(String, Vec<String>)> = module_map.into_iter().collect();
        modules.sort_by(|a, b| a.0.cmp(&b.0));

        // Collect key types (public structs/enums/traits)
        let key_types: Vec<String> = summary
            .public_only()
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    ElementKind::Struct
                        | ElementKind::Enum
                        | ElementKind::Trait
                        | ElementKind::Interface
                )
            })
            .map(|e| e.name.clone())
            .take(20)
            .collect();

        Ok(Some(ProjectOverview {
            path: proj.path.display().to_string(),
            languages: proj.languages.clone(),
            file_count: proj.file_count,
            lines_of_code: proj.lines_of_code,
            symbol_count: proj.symbol_count,
            modules,
            key_types,
        }))
    }

    // ========================================================================
    // Internal
    // ========================================================================

    fn load_summary(&mut self, hash: &str) -> Result<&ProjectSummary, CodeIndexError> {
        if !self.cache.contains_key(hash) {
            let cidx_path = self.index_dir.join(format!("{}.cidx", hash));
            if !cidx_path.exists() {
                return Err(CodeIndexError::NotFound(hash.to_string()));
            }
            let summary = persistence::load_summary(&cidx_path)
                .map_err(|e| CodeIndexError::Persist(e.to_string()))?;
            self.cache.insert(hash.to_string(), summary);
        }
        Ok(self.cache.get(hash).expect("just inserted or existed"))
    }
}

/// Hash a path to a stable identifier (same approach as IndexManager)
fn hash_path(path: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum CodeIndexError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Scan error: {0}")]
    Scan(String),

    #[error("Persistence error: {0}")]
    Persist(String),

    #[error("Project not found: {0}")]
    NotFound(String),
}

#[cfg(test)]
#[path = "code_index_test.rs"]
mod code_index_test;
