use crate::SharedIndex;

/// Build the system prompt, dynamically reflecting index availability.
///
/// When the index is loaded, includes file count and indexed roots.
/// When it's still loading, tells the model which tools work now.
pub fn system_prompt(index: &SharedIndex) -> String {
    let index_info = match index.read() {
        Ok(guard) => match guard.as_ref() {
            Some(mgr) => {
                let count = mgr.total_file_count() as u64;
                let roots = mgr.roots();
                let roots_display = if roots.is_empty() {
                    "none yet".to_string()
                } else {
                    roots
                        .iter()
                        .map(|r| abbreviate_home(r))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                format!("You have {count} files indexed across: {roots_display}")
            }
            None => "The search index is still loading. list_directory and file_info work now; \
                     search_files and search_by_type will return a notice until indexing completes."
                .to_string(),
        },
        Err(_) => "Index unavailable.".to_string(),
    };

    format!(
        "You are Zero's built-in file assistant. Zero is a privacy-first Finder & Spotlight \
         replacement for macOS. All search is 100% local — nothing leaves the machine.\n\
         \n\
         {index_info}\n\
         \n\
         Available tools:\n\
         - search_files: full-text search by filename or path fragment\n\
         - search_by_type: filter by category (images, videos, audio, documents, code, \
           archives, fonts, applications, folders)\n\
         - list_directory: browse a directory's contents\n\
         - file_info: get metadata for a specific file\n\
         \n\
         Guidelines:\n\
         - All operations are read-only and local.\n\
         - Use ~ prefix when displaying paths (e.g. ~/Documents instead of /Users/name/Documents).\n\
         - If a search returns no results, suggest alternative queries or broader type filters.\n\
         - Be concise. Show paths and sizes in your answers."
    )
}

fn abbreviate_home(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str: String = home.to_string_lossy().into_owned();
        if let Some(rest) = path.strip_prefix(&home_str) {
            if rest.is_empty() {
                return "~".to_string();
            }
            return format!("~{rest}");
        }
    }
    path.to_string()
}
