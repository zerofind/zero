use super::*;

#[test]
fn test_language_as_str() {
    assert_eq!(Language::Rust.as_str(), "rust");
    assert_eq!(Language::Go.as_str(), "go");
}

#[test]
fn test_element_kind_as_str() {
    assert_eq!(ElementKind::Function.as_str(), "fn");
    assert_eq!(ElementKind::Struct.as_str(), "struct");
    assert_eq!(ElementKind::Trait.as_str(), "trait");
}

#[test]
fn test_element_kind_from_str_loose() {
    assert_eq!(
        ElementKind::from_str_loose("function"),
        Some(ElementKind::Function)
    );
    assert_eq!(
        ElementKind::from_str_loose("fn"),
        Some(ElementKind::Function)
    );
    assert_eq!(
        ElementKind::from_str_loose("struct"),
        Some(ElementKind::Struct)
    );
    assert_eq!(
        ElementKind::from_str_loose("TRAIT"),
        Some(ElementKind::Trait)
    );
    assert_eq!(ElementKind::from_str_loose("unknown"), None);
}

#[test]
fn test_visibility_is_public() {
    assert!(Visibility::Public.is_public());
    assert!(!Visibility::Private.is_public());
    assert!(!Visibility::PublicCrate.is_public());
    assert!(!Visibility::PublicSuper.is_public());
}

#[test]
fn test_project_summary_public_only() {
    let mut summary = ProjectSummary::new("test".to_string());
    summary.elements.push(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Function,
        name: "pub_fn".to_string(),
        signature: "pub fn pub_fn()".to_string(),
        file_path: "src/lib.rs".to_string(),
        line_number: 1,
        doc: None,
        visibility: Visibility::Public,
    });
    summary.elements.push(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Function,
        name: "priv_fn".to_string(),
        signature: "fn priv_fn()".to_string(),
        file_path: "src/lib.rs".to_string(),
        line_number: 5,
        doc: None,
        visibility: Visibility::Private,
    });

    let public = summary.public_only();
    assert_eq!(public.len(), 1);
    assert_eq!(public[0].name, "pub_fn");
}

#[test]
fn test_project_summary_merge() {
    let mut base = ProjectSummary::new("test".to_string());
    base.elements.push(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Function,
        name: "old_fn".to_string(),
        signature: "fn old_fn()".to_string(),
        file_path: "src/lib.rs".to_string(),
        line_number: 1,
        doc: None,
        visibility: Visibility::Public,
    });
    base.elements.push(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Function,
        name: "keep_fn".to_string(),
        signature: "fn keep_fn()".to_string(),
        file_path: "src/other.rs".to_string(),
        line_number: 1,
        doc: None,
        visibility: Visibility::Public,
    });

    let mut update = ProjectSummary::new("test".to_string());
    update.elements.push(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Function,
        name: "new_fn".to_string(),
        signature: "fn new_fn()".to_string(),
        file_path: "src/lib.rs".to_string(),
        line_number: 1,
        doc: None,
        visibility: Visibility::Public,
    });

    base.merge(update);

    assert_eq!(base.elements.len(), 2);
    let names: Vec<&str> = base.elements.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"new_fn"));
    assert!(names.contains(&"keep_fn"));
    assert!(!names.contains(&"old_fn"));
}

#[test]
fn test_project_summary_languages() {
    let mut summary = ProjectSummary::new("test".to_string());
    summary.elements.push(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Function,
        name: "f".to_string(),
        signature: "fn f()".to_string(),
        file_path: "src/lib.rs".to_string(),
        line_number: 1,
        doc: None,
        visibility: Visibility::Public,
    });
    summary.elements.push(CodeElement {
        language: Language::Go,
        kind: ElementKind::Function,
        name: "F".to_string(),
        signature: "func F()".to_string(),
        file_path: "main.go".to_string(),
        line_number: 1,
        doc: None,
        visibility: Visibility::Public,
    });

    let langs = summary.languages();
    assert_eq!(langs.len(), 2);
}
