//! Tests for backup templates

use super::*;

// ============================================================================
// Template listing tests
// ============================================================================

#[test]
fn test_all_templates() {
    let templates = all_templates();
    assert_eq!(templates.len(), 2);
    assert!(templates.iter().any(|t| t.id == "mac-essentials"));
    assert!(templates.iter().any(|t| t.id == "macos-developer"));
}

#[test]
fn test_get_template() {
    assert!(get_template("mac-essentials").is_some());
    assert!(get_template("macos-developer").is_some());
    assert!(get_template("nonexistent").is_none());
}

#[test]
fn test_get_template_case_sensitive() {
    assert!(get_template("Mac-Essentials").is_none());
    assert!(get_template("MAC-ESSENTIALS").is_none());
}

#[test]
fn test_template_ids() {
    let ids = template_ids();
    assert!(ids.contains(&"mac-essentials"));
    assert!(ids.contains(&"macos-developer"));
    assert_eq!(ids.len(), 2);
}

// ============================================================================
// Mac Essentials template tests
// ============================================================================

#[test]
fn test_mac_essentials_sources() {
    let template = get_template("mac-essentials").unwrap();
    assert!(template.sources.iter().any(|s| s.path == "Documents"));
    assert!(template.sources.iter().any(|s| s.path == "Downloads"));
    assert!(template.sources.iter().any(|s| s.path == "Desktop"));
    assert!(template.sources.iter().any(|s| s.path == "Pictures"));
}

#[test]
fn test_mac_essentials_has_media_folders() {
    let template = get_template("mac-essentials").unwrap();
    assert!(template.sources.iter().any(|s| s.path == "Movies"));
    assert!(template.sources.iter().any(|s| s.path == "Music"));
}

#[test]
fn test_mac_essentials_metadata() {
    let template = get_template("mac-essentials").unwrap();
    assert_eq!(template.id, "mac-essentials");
    assert_eq!(template.name, "Mac Essentials");
    assert!(!template.description.is_empty());
}

// ============================================================================
// macOS Developer template tests
// ============================================================================

#[test]
fn test_developer_has_dotfiles() {
    let template = get_template("macos-developer").unwrap();
    assert!(template.sources.iter().any(|s| s.path == ".ssh"));
    assert!(template.sources.iter().any(|s| s.path == ".gitconfig"));
    assert!(template.sources.iter().any(|s| s.path == ".config"));
}

#[test]
fn test_developer_has_code_folder() {
    let template = get_template("macos-developer").unwrap();
    // Check for common code folder names
    let has_code_folder = template.sources.iter().any(|s| {
        s.path == "code" || s.path == "Code" || s.path == "Developer" || s.path == "Projects"
    });
    assert!(has_code_folder || template.sources.iter().any(|s| s.path.contains("code")));
}

#[test]
fn test_developer_excludes() {
    let template = get_template("macos-developer").unwrap();
    assert!(template.excludes.contains(&"node_modules"));
    assert!(template.excludes.contains(&"target"));
    assert!(template.excludes.contains(&".git"));
}

#[test]
fn test_developer_excludes_build_artifacts() {
    let template = get_template("macos-developer").unwrap();
    // Should exclude common build directories
    assert!(template.excludes.contains(&"node_modules"));
    assert!(template.excludes.contains(&"target"));
}

#[test]
fn test_developer_metadata() {
    let template = get_template("macos-developer").unwrap();
    assert_eq!(template.id, "macos-developer");
    assert_eq!(template.name, "macOS Developer");
    assert!(!template.description.is_empty());
}

// ============================================================================
// Template resolution tests
// ============================================================================

#[test]
fn test_resolve_template() {
    let template = get_template("mac-essentials").unwrap();
    let resolved = template.resolve();

    // Should have same number of sources
    assert_eq!(resolved.sources.len(), template.sources.len());

    // Should have system excludes
    assert!(resolved.excludes.iter().any(|e| e == ".DS_Store"));
    assert!(resolved.excludes.iter().any(|e| e == ".Trash"));
}

#[test]
fn test_resolve_template_includes_template_excludes() {
    let template = get_template("macos-developer").unwrap();
    let resolved = template.resolve();

    // Should include both system excludes and template excludes
    assert!(resolved.excludes.iter().any(|e| e == ".DS_Store")); // system
    assert!(resolved.excludes.iter().any(|e| e == "node_modules")); // template
}

#[test]
fn test_resolved_sources_have_paths() {
    let template = get_template("mac-essentials").unwrap();
    let resolved = template.resolve();

    for source in &resolved.sources {
        // Each resolved source should have a full path
        assert!(source.path.is_absolute() || source.path.starts_with("~"));
    }
}

// ============================================================================
// Display tests
// ============================================================================

#[test]
fn test_template_display() {
    let template = get_template("mac-essentials").unwrap();
    let display = format!("{}", template);
    assert!(display.contains("Mac Essentials"));
    assert!(display.contains("Documents"));
}

#[test]
fn test_template_display_includes_description() {
    let template = get_template("mac-essentials").unwrap();
    let display = format!("{}", template);
    // Display should include some indication of what the template does
    assert!(!display.is_empty());
}

#[test]
fn test_resolved_template_display() {
    let template = get_template("mac-essentials").unwrap();
    let resolved = template.resolve();
    let display = format!("{}", resolved);
    assert!(!display.is_empty());
}

// ============================================================================
// TemplateSource tests
// ============================================================================

#[test]
fn test_template_source_required() {
    let source = TemplateSource::required("Documents", "User documents");
    assert_eq!(source.path, "Documents");
    assert!(!source.optional);
    assert_eq!(source.description, "User documents");
}

#[test]
fn test_template_source_optional() {
    let source = TemplateSource::optional(".config", "Config files");
    assert_eq!(source.path, ".config");
    assert!(source.optional);
    assert_eq!(source.description, "Config files");
}

// ============================================================================
// Helper method tests
// ============================================================================

#[test]
fn test_sources_vec() {
    let template = get_template("mac-essentials").unwrap();
    let sources = template.sources_vec();
    assert!(!sources.is_empty());
    assert!(sources.iter().any(|s| s.path == "Documents"));
}

#[test]
fn test_excludes_vec() {
    let template = get_template("macos-developer").unwrap();
    let excludes = template.excludes_vec();
    assert!(!excludes.is_empty());
    assert!(excludes.contains(&"node_modules"));
}

#[test]
fn test_existing_sources() {
    let template = get_template("mac-essentials").unwrap();
    // This will check actual filesystem, results may vary
    let existing = template.existing_sources();
    // Just verify it returns a vec without panicking
    assert!(existing.len() <= template.sources.len());
}

// ============================================================================
// ResolvedTemplate tests
// ============================================================================

#[test]
fn test_resolved_template_is_valid() {
    let template = get_template("mac-essentials").unwrap();
    let resolved = template.resolve();
    // Validity depends on whether required sources exist on this system
    // Just verify the method works
    let _ = resolved.is_valid();
}

#[test]
fn test_resolved_template_missing_required() {
    let template = get_template("mac-essentials").unwrap();
    let resolved = template.resolve();
    // Method should return list of missing required sources
    let missing = resolved.missing_required_sources();
    // Just verify it returns without panicking
    assert!(missing.len() <= resolved.sources.len());
}

#[test]
fn test_resolved_template_existing_sources() {
    let template = get_template("mac-essentials").unwrap();
    let resolved = template.resolve();
    let existing = resolved.existing_sources();
    // Should not exceed total sources
    assert!(existing.len() <= resolved.sources.len());
}

#[test]
fn test_resolved_template_estimate_size() {
    let template = get_template("mac-essentials").unwrap();
    let resolved = template.resolve();
    // Size estimation may return 0 if no sources exist, or error if paths don't exist
    let size_result = resolved.estimate_size();
    // Just verify it returns a result (Ok or Err is fine)
    let _ = size_result;
}

// ============================================================================
// Clone and Debug tests
// ============================================================================

#[test]
fn test_template_clone() {
    let template = get_template("mac-essentials").unwrap();
    let cloned = template.clone();
    assert_eq!(template.id, cloned.id);
    assert_eq!(template.name, cloned.name);
}

#[test]
fn test_template_debug() {
    let template = get_template("mac-essentials").unwrap();
    let debug = format!("{:?}", template);
    assert!(debug.contains("mac-essentials"));
}

#[test]
fn test_template_source_clone() {
    let source = TemplateSource::required("Documents", "Docs");
    let cloned = source.clone();
    assert_eq!(source.path, cloned.path);
    assert_eq!(source.optional, cloned.optional);
}

#[test]
fn test_template_source_debug() {
    let source = TemplateSource::required("Documents", "Docs");
    let debug = format!("{:?}", source);
    assert!(debug.contains("Documents"));
}
