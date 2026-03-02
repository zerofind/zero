use std::path::Path;

/// Max file size we'll load into the editor (1 MB).
pub const MAX_EDITABLE_SIZE: u64 = 1_048_576;

/// Extensions we consider text-editable.
const EDITABLE_EXTENSIONS: &[&str] = &[
    // Code
    "rs", "py", "js", "ts", "jsx", "tsx", "go", "c", "h", "cpp", "hpp",
    "java", "kt", "swift", "rb", "php", "lua", "sh", "bash", "zsh", "fish",
    // Config / data
    "toml", "yaml", "yml", "json", "xml", "csv", "ini", "cfg", "conf",
    "env", "properties",
    // Docs / text
    "md", "txt", "rst", "log", "adoc",
    // Web
    "html", "htm", "css", "scss", "sass", "less", "svg",
    // Other
    "sql", "graphql", "proto", "dockerfile", "makefile",
    "gitignore", "editorconfig",
];

/// Check whether a file path is a text-editable file.
pub fn is_editable(path: &Path) -> bool {
    // Check file size first
    if let Ok(meta) = std::fs::metadata(path)
        && meta.len() > MAX_EDITABLE_SIZE
    {
        return false;
    }

    // Dotfiles without extension (e.g., .gitignore, .env)
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let lower = name.to_lowercase();
        if EDITABLE_EXTENSIONS.iter().any(|ext| lower == format!(".{ext}"))
            || lower == "makefile"
            || lower == "dockerfile"
            || lower == "cmakelists.txt"
        {
            return true;
        }
    }

    // Extension check
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_lowercase(),
        None => return false,
    };

    EDITABLE_EXTENSIONS.contains(&ext.as_str())
}
