//! CLI commands for code indexing

use std::path::{Path, PathBuf};

use code::{CodeIndex, CodeSearchOpts, ElementKind, Language};
use foundation::output::Outputter;

pub fn cmd_code_index(out: &Outputter, path: &Path, git_only: bool) -> anyhow::Result<()> {
    let mut index = CodeIndex::new()?;

    // Check if path is a single project (has .git or Cargo.toml/go.mod)
    let is_single_project = path.join(".git").is_dir()
        || path.join("Cargo.toml").is_file()
        || path.join("go.mod").is_file();

    if is_single_project {
        out.info(&format!("Indexing project: {}", path.display()));
        let project = index.index_project(path)?;
        out.success(&format!(
            "Indexed {} symbols in {} files ({} LOC)",
            project.symbol_count, project.file_count, project.lines_of_code
        ));
    } else {
        out.info(&format!("Discovering projects under: {}", path.display()));
        let indexed = index.index_all(path, git_only)?;
        if indexed.is_empty() {
            out.warn("No projects found");
        } else {
            out.success(&format!("Indexed {} projects:", indexed.len()));
            for p in &indexed {
                out.info(&format!("  {p}"));
            }
        }
    }

    Ok(())
}

pub fn cmd_code_search(
    out: &Outputter,
    query: &str,
    kind: Option<&str>,
    language: Option<&str>,
    project: Option<&PathBuf>,
    limit: usize,
) -> anyhow::Result<()> {
    let mut index = CodeIndex::new()?;

    let kind_filter = kind.and_then(ElementKind::from_str_loose);
    let lang_filter = language.and_then(|l| match l.to_lowercase().as_str() {
        "rust" | "rs" => Some(Language::Rust),
        "go" => Some(Language::Go),
        _ => None,
    });

    let opts = CodeSearchOpts {
        kind: kind_filter,
        language: lang_filter,
        project: project.cloned(),
        limit,
    };

    let results = index.search(query, &opts)?;

    if results.is_empty() {
        out.info(&format!("No symbols matching \"{query}\""));
        return Ok(());
    }

    out.info(&format!(
        "Found {} symbols matching \"{}\":\n",
        results.len(),
        query
    ));

    for r in &results {
        let e = &r.element;
        let doc_str = e
            .doc
            .as_ref()
            .map(|d| {
                let first = d.lines().next().unwrap_or("");
                format!("  /// {first}")
            })
            .unwrap_or_default();

        println!("{}", e.signature);
        println!(
            "  {}:{}  [{}]  {}  {}",
            e.file_path,
            e.line_number,
            short_project_name(&r.project_path),
            e.language,
            e.kind
        );
        if !doc_str.is_empty() {
            println!("{doc_str}");
        }
        println!();
    }

    Ok(())
}

pub fn cmd_code_list(out: &Outputter) -> anyhow::Result<()> {
    let index = CodeIndex::new()?;
    let projects = index.indexed_projects();

    if projects.is_empty() {
        out.info("No indexed code projects.");
        out.info("Run: zero code index <path>");
        return Ok(());
    }

    out.info(&format!("Indexed code projects ({}):\n", projects.len()));
    for p in &projects {
        let langs: Vec<&str> = p.languages.iter().map(code::Language::as_str).collect();
        println!(
            "{}    {}    {} symbols    {} files    {} LOC",
            p.path.display(),
            langs.join(", "),
            p.symbol_count,
            p.file_count,
            p.lines_of_code,
        );
    }

    Ok(())
}

pub fn cmd_code_overview(out: &Outputter, path: &Path) -> anyhow::Result<()> {
    let mut index = CodeIndex::new()?;

    let Some(overview) = index.overview(path)? else {
        out.warn(&format!("Project not indexed: {}", path.display()));
        out.info("Run: zero code index <path>");
        return Ok(());
    };

    let langs: Vec<&str> = overview
        .languages
        .iter()
        .map(code::Language::as_str)
        .collect();

    println!("Project: {}", overview.path);
    println!("Languages: {}", langs.join(", "));
    println!("Files: {} source files", overview.file_count);
    println!("Lines: {} LOC", overview.lines_of_code);
    println!("Symbols: {} public", overview.symbol_count);

    if !overview.modules.is_empty() {
        println!("\nModules:");
        for (module, types) in &overview.modules {
            if types.is_empty() {
                println!("  {module}");
            } else {
                println!("  {} — ({})", module, types.join(", "));
            }
        }
    }

    if !overview.key_types.is_empty() {
        println!("\nKey types: {}", overview.key_types.join(", "));
    }

    Ok(())
}

pub fn cmd_code_remove(out: &Outputter, path: &Path) -> anyhow::Result<()> {
    let mut index = CodeIndex::new()?;
    index.remove_project(path)?;
    out.success(&format!("Removed: {}", path.display()));
    Ok(())
}

pub fn cmd_code_project_symbols(
    out: &Outputter,
    project: &Path,
    kind: Option<&str>,
    limit: usize,
) -> anyhow::Result<()> {
    let mut index = CodeIndex::new()?;

    let kind_filter = kind.and_then(ElementKind::from_str_loose);
    let opts = CodeSearchOpts {
        kind: kind_filter,
        limit,
        ..Default::default()
    };

    let results = index.project_symbols(project, &opts)?;

    if results.is_empty() {
        out.info("No symbols found");
        return Ok(());
    }

    // SAFETY(index): results is guaranteed non-empty by the is_empty() check above
    let proj_path = &results
        .first()
        .expect("results non-empty after is_empty check")
        .project_path;
    let langs: Vec<&str> = index
        .indexed_projects()
        .iter()
        .find(|p| p.path.display().to_string() == *proj_path)
        .map(|p| p.languages.iter().map(code::Language::as_str).collect())
        .unwrap_or_default();

    out.info(&format!(
        "{} — {} public symbols ({})\n",
        proj_path,
        results.len(),
        langs.join(", ")
    ));

    let mut current_file = String::new();
    for r in &results {
        let e = &r.element;
        if e.file_path != current_file {
            current_file.clone_from(&e.file_path);
            println!("// {current_file}");
        }
        println!("{}", e.signature);
    }

    Ok(())
}

fn short_project_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().to_string())
}
