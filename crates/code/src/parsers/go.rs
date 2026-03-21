//! Go source file parser using tree-sitter
//!
//! Extracts public types, functions, interfaces, and methods from Go source files.

use std::fs;
use std::path::Path;

use tree_sitter::{Node, Parser};

use super::super::element::{CodeElement, ElementKind, Language, Visibility};

/// Parse a Go source file and extract code elements
pub fn parse_file(path: &Path, relative_path: &str) -> Result<Vec<CodeElement>, ParseError> {
    let content = fs::read_to_string(path).map_err(|e| ParseError::IoError {
        path: path.display().to_string(),
        source: e,
    })?;

    parse_source(&content, relative_path)
}

/// Parse Go source code string
pub fn parse_source(source: &str, file_path: &str) -> Result<Vec<CodeElement>, ParseError> {
    let mut parser = Parser::new();
    let language = tree_sitter_go::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| ParseError::TreeSitterError {
            path: file_path.to_string(),
            message: e.to_string(),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ParseError::TreeSitterError {
            path: file_path.to_string(),
            message: "Failed to parse".to_string(),
        })?;

    let mut elements = Vec::new();
    let root = tree.root_node();

    extract_elements(&root, source, file_path, &mut elements);

    Ok(elements)
}

/// Extract code elements from tree-sitter AST
fn extract_elements(node: &Node, source: &str, file_path: &str, elements: &mut Vec<CodeElement>) {
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(elem) = extract_function(&child, source, file_path) {
                    elements.push(elem);
                }
            }
            "method_declaration" => {
                if let Some(elem) = extract_method(&child, source, file_path) {
                    elements.push(elem);
                }
            }
            "type_declaration" => {
                extract_type_declaration(&child, source, file_path, elements);
            }
            "const_declaration" => {
                extract_const_declaration(&child, source, file_path, elements);
            }
            "var_declaration" => {
                extract_var_declaration(&child, source, file_path, elements);
            }
            _ => {
                extract_elements(&child, source, file_path, elements);
            }
        }
    }
}

fn extract_function(node: &Node, source: &str, file_path: &str) -> Option<CodeElement> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);

    let visibility = go_visibility(&name);
    let doc = get_preceding_comment(node, source);
    let line_number = node.start_position().row + 1;
    let signature = format_function_signature(node, source, &name);

    Some(CodeElement {
        language: Language::Go,
        kind: ElementKind::Function,
        name,
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility,
    })
}

fn extract_method(node: &Node, source: &str, file_path: &str) -> Option<CodeElement> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);

    let visibility = go_visibility(&name);
    let doc = get_preceding_comment(node, source);
    let line_number = node.start_position().row + 1;

    let receiver = node
        .child_by_field_name("receiver")
        .and_then(|r| get_receiver_type(&r, source));

    let signature = format_method_signature(node, source, &name, receiver.as_deref());

    Some(CodeElement {
        language: Language::Go,
        kind: ElementKind::Method,
        name,
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility,
    })
}

fn extract_type_declaration(
    node: &Node,
    source: &str,
    file_path: &str,
    elements: &mut Vec<CodeElement>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_spec"
            && let Some(elem) = extract_type_spec(&child, source, file_path)
        {
            elements.push(elem);
        }
    }
}

fn extract_type_spec(node: &Node, source: &str, file_path: &str) -> Option<CodeElement> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);

    let visibility = go_visibility(&name);
    let doc = get_preceding_comment(node, source);
    let line_number = node.start_position().row + 1;

    let type_node = node.child_by_field_name("type")?;
    let (kind, signature) = match type_node.kind() {
        "struct_type" => {
            let sig = format_struct_signature(&name, &type_node, source);
            (ElementKind::Struct, sig)
        }
        "interface_type" => {
            let sig = format_interface_signature(&name, &type_node, source);
            (ElementKind::Interface, sig)
        }
        _ => {
            let type_str = get_node_text(&type_node, source);
            (ElementKind::TypeAlias, format!("type {name} = {type_str}"))
        }
    };

    Some(CodeElement {
        language: Language::Go,
        kind,
        name,
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility,
    })
}

fn extract_const_declaration(
    node: &Node,
    source: &str,
    file_path: &str,
    elements: &mut Vec<CodeElement>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "const_spec"
            && let Some(elem) = extract_const_spec(&child, source, file_path)
        {
            elements.push(elem);
        }
    }
}

fn extract_const_spec(node: &Node, source: &str, file_path: &str) -> Option<CodeElement> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);

    let visibility = go_visibility(&name);
    let doc = get_preceding_comment(node, source);
    let line_number = node.start_position().row + 1;

    let type_str = node
        .child_by_field_name("type")
        .map(|t| get_node_text(&t, source))
        .unwrap_or_default();

    let signature = if type_str.is_empty() {
        format!("const {name}")
    } else {
        format!("const {name} {type_str}")
    };

    Some(CodeElement {
        language: Language::Go,
        kind: ElementKind::Const,
        name,
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility,
    })
}

fn extract_var_declaration(
    node: &Node,
    source: &str,
    file_path: &str,
    elements: &mut Vec<CodeElement>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "var_spec"
            && let Some(elem) = extract_var_spec(&child, source, file_path)
        {
            elements.push(elem);
        }
    }
}

fn extract_var_spec(node: &Node, source: &str, file_path: &str) -> Option<CodeElement> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);

    let visibility = go_visibility(&name);
    let doc = get_preceding_comment(node, source);
    let line_number = node.start_position().row + 1;

    let type_str = node
        .child_by_field_name("type")
        .map(|t| get_node_text(&t, source))
        .unwrap_or_default();

    let signature = if type_str.is_empty() {
        format!("var {name}")
    } else {
        format!("var {name} {type_str}")
    };

    Some(CodeElement {
        language: Language::Go,
        kind: ElementKind::Static, // Use Static for package-level vars
        name,
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility,
    })
}

// ============================================================================
// Signature formatting
// ============================================================================

fn format_function_signature(node: &Node, source: &str, name: &str) -> String {
    let params = node
        .child_by_field_name("parameters")
        .map_or_else(|| "()".to_string(), |p| get_node_text(&p, source));

    let result = node
        .child_by_field_name("result")
        .map(|r| format!(" {}", get_node_text(&r, source)))
        .unwrap_or_default();

    format!("func {name}{params}{result}")
}

fn format_method_signature(
    node: &Node,
    source: &str,
    name: &str,
    receiver: Option<&str>,
) -> String {
    let params = node
        .child_by_field_name("parameters")
        .map_or_else(|| "()".to_string(), |p| get_node_text(&p, source));

    let result = node
        .child_by_field_name("result")
        .map(|r| format!(" {}", get_node_text(&r, source)))
        .unwrap_or_default();

    if let Some(recv) = receiver {
        format!("func ({recv}) {name}{params}{result}")
    } else {
        format!("func {name}{params}{result}")
    }
}

fn format_struct_signature(name: &str, type_node: &Node, source: &str) -> String {
    let mut fields = Vec::new();

    let mut cursor = type_node.walk();
    for child in type_node.children(&mut cursor) {
        if child.kind() == "field_declaration_list" {
            let mut field_cursor = child.walk();
            for field in child.children(&mut field_cursor) {
                if field.kind() == "field_declaration" {
                    let field_names: Vec<String> = field
                        .children_by_field_name("name", &mut field.walk())
                        .map(|n| get_node_text(&n, source))
                        .collect();

                    let field_type = field
                        .child_by_field_name("type")
                        .map(|t| get_node_text(&t, source))
                        .unwrap_or_default();

                    if !field_names.is_empty() && !field_type.is_empty() {
                        fields.push(format!("{} {}", field_names.join(", "), field_type));
                    } else if !field_type.is_empty() {
                        fields.push(field_type);
                    }
                }
            }
        }
    }

    if fields.is_empty() {
        format!("type {name} struct {{}}")
    } else {
        format!("type {} struct {{ {} }}", name, fields.join("; "))
    }
}

fn format_interface_signature(name: &str, type_node: &Node, source: &str) -> String {
    let mut methods = Vec::new();

    let mut cursor = type_node.walk();
    for child in type_node.children(&mut cursor) {
        if child.kind() == "method_spec" {
            let method_name = child
                .child_by_field_name("name")
                .map(|n| get_node_text(&n, source))
                .unwrap_or_default();

            let params = child
                .child_by_field_name("parameters")
                .map_or_else(|| "()".to_string(), |p| get_node_text(&p, source));

            let result = child
                .child_by_field_name("result")
                .map(|r| format!(" {}", get_node_text(&r, source)))
                .unwrap_or_default();

            if !method_name.is_empty() {
                methods.push(format!("{method_name}{params}{result}"));
            }
        } else if child.kind() == "type_identifier" || child.kind() == "qualified_type" {
            methods.push(get_node_text(&child, source));
        }
    }

    if methods.is_empty() {
        format!("type {name} interface {{}}")
    } else {
        format!("type {} interface {{ {} }}", name, methods.join("; "))
    }
}

// ============================================================================
// Helper functions
// ============================================================================

fn go_visibility(name: &str) -> Visibility {
    if name.chars().next().is_some_and(char::is_uppercase) {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

fn get_node_text(node: &Node, source: &str) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    source[start..end].to_string()
}

fn get_receiver_type(receiver_node: &Node, source: &str) -> Option<String> {
    let mut cursor = receiver_node.walk();
    for child in receiver_node.children(&mut cursor) {
        if child.kind() == "parameter_declaration"
            && let Some(type_node) = child.child_by_field_name("type")
        {
            return Some(get_node_text(&type_node, source));
        }
    }
    None
}

fn get_preceding_comment(node: &Node, source: &str) -> Option<String> {
    let mut prev = node.prev_sibling();
    let mut comments = Vec::new();

    while let Some(sibling) = prev {
        if sibling.kind() == "comment" {
            let text = get_node_text(&sibling, source);
            let cleaned = text
                .trim_start_matches("//")
                .trim_start_matches("/*")
                .trim_end_matches("*/")
                .trim();
            comments.push(cleaned.to_string());
            prev = sibling.prev_sibling();
        } else {
            break;
        }
    }

    if comments.is_empty() {
        None
    } else {
        comments.reverse();
        Some(comments.join("\n"))
    }
}

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Failed to read file {path}: {source}")]
    IoError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse {path}: {message}")]
    TreeSitterError { path: String, message: String },
}

#[cfg(test)]
#[path = "go_parser_test.rs"]
mod go_parser_test;
