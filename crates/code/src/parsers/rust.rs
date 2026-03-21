//! Rust source file parser using syn
//!
//! Extracts public types, functions, traits, and impl blocks from Rust source files.

use std::fs;
use std::path::Path;

use syn::{
    Attribute, Fields, ImplItem, Item, ItemConst, ItemEnum, ItemFn, ItemImpl, ItemMod, ItemStatic,
    ItemStruct, ItemTrait, ItemType, TraitItem, TraitItemConst, TraitItemType,
};

use super::super::element::{CodeElement, ElementKind, Language, Visibility};
use super::rust_format::{
    convert_visibility, format_fn_params, format_generics, format_generics_where,
    format_return_type, format_type, format_type_param_bounds, format_visibility,
};

/// Parse a Rust source file and extract code elements
pub fn parse_file(path: &Path, relative_path: &str) -> Result<Vec<CodeElement>, ParseError> {
    let content = fs::read_to_string(path).map_err(|e| ParseError::IoError {
        path: path.display().to_string(),
        source: e,
    })?;

    parse_source(&content, relative_path)
}

/// Parse Rust source code string
pub fn parse_source(source: &str, file_path: &str) -> Result<Vec<CodeElement>, ParseError> {
    let syntax = syn::parse_file(source).map_err(|e| ParseError::SynError {
        path: file_path.to_string(),
        message: e.to_string(),
    })?;

    let mut elements = Vec::new();

    for item in syntax.items {
        extract_item(&item, file_path, &mut elements);
    }

    Ok(elements)
}

/// Extract code elements from a syn Item
fn extract_item(item: &Item, file_path: &str, elements: &mut Vec<CodeElement>) {
    match item {
        Item::Fn(item_fn) => {
            if let Some(elem) = extract_function(item_fn, file_path) {
                elements.push(elem);
            }
        }
        Item::Struct(item_struct) => {
            if let Some(elem) = extract_struct(item_struct, file_path) {
                elements.push(elem);
            }
        }
        Item::Enum(item_enum) => {
            if let Some(elem) = extract_enum(item_enum, file_path) {
                elements.push(elem);
            }
        }
        Item::Trait(item_trait) => {
            extract_trait(item_trait, file_path, elements);
        }
        Item::Impl(item_impl) => {
            extract_impl(item_impl, file_path, elements);
        }
        Item::Const(item_const) => {
            if let Some(elem) = extract_const(item_const, file_path) {
                elements.push(elem);
            }
        }
        Item::Static(item_static) => {
            if let Some(elem) = extract_static(item_static, file_path) {
                elements.push(elem);
            }
        }
        Item::Type(item_type) => {
            if let Some(elem) = extract_type_alias(item_type, file_path) {
                elements.push(elem);
            }
        }
        Item::Mod(item_mod) => {
            if let Some(elem) = extract_module(item_mod, file_path) {
                elements.push(elem);
            }
        }
        _ => {}
    }
}

fn extract_function(item: &ItemFn, file_path: &str) -> Option<CodeElement> {
    let visibility = convert_visibility(&item.vis);
    let name = item.sig.ident.to_string();
    let doc = extract_doc_comment(&item.attrs);
    let line_number = get_line_number(&item.sig.fn_token.span);
    let signature = format_function_signature(item);

    Some(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Function,
        name,
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility,
    })
}

fn extract_struct(item: &ItemStruct, file_path: &str) -> Option<CodeElement> {
    let visibility = convert_visibility(&item.vis);
    let name = item.ident.to_string();
    let doc = extract_doc_comment(&item.attrs);
    let line_number = get_line_number(&item.struct_token.span);
    let signature = format_struct_signature(item);

    Some(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Struct,
        name,
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility,
    })
}

fn extract_enum(item: &ItemEnum, file_path: &str) -> Option<CodeElement> {
    let visibility = convert_visibility(&item.vis);
    let name = item.ident.to_string();
    let doc = extract_doc_comment(&item.attrs);
    let line_number = get_line_number(&item.enum_token.span);
    let signature = format_enum_signature(item);

    Some(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Enum,
        name,
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility,
    })
}

fn extract_trait(item: &ItemTrait, file_path: &str, elements: &mut Vec<CodeElement>) {
    let visibility = convert_visibility(&item.vis);
    let name = item.ident.to_string();
    let doc = extract_doc_comment(&item.attrs);
    let line_number = get_line_number(&item.trait_token.span);
    let signature = format_trait_signature(item);

    elements.push(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Trait,
        name: name.clone(),
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility,
    });

    for trait_item in &item.items {
        match trait_item {
            TraitItem::Fn(method) => {
                let method_doc = extract_doc_comment(&method.attrs);
                let method_line = get_line_number(&method.sig.fn_token.span);
                let method_sig = format_trait_method_signature(method, &name);

                elements.push(CodeElement {
                    language: Language::Rust,
                    kind: ElementKind::Function,
                    name: method.sig.ident.to_string(),
                    signature: method_sig,
                    file_path: file_path.to_string(),
                    line_number: method_line,
                    doc: method_doc,
                    visibility: Visibility::Public,
                });
            }
            TraitItem::Type(assoc_type) => {
                if let Some(elem) = extract_trait_assoc_type(assoc_type, &name, file_path) {
                    elements.push(elem);
                }
            }
            TraitItem::Const(assoc_const) => {
                if let Some(elem) = extract_trait_assoc_const(assoc_const, &name, file_path) {
                    elements.push(elem);
                }
            }
            _ => {}
        }
    }
}

fn extract_impl(item: &ItemImpl, file_path: &str, elements: &mut Vec<CodeElement>) {
    let impl_type = format_type(&item.self_ty);
    let trait_name = item.trait_.as_ref().map(|(_, path, _)| {
        path.segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::")
    });

    let line_number = get_line_number(&item.impl_token.span);

    let impl_sig = if let Some(ref trait_name) = trait_name {
        format!("impl {trait_name} for {impl_type}")
    } else {
        format!("impl {impl_type}")
    };

    elements.push(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Impl,
        name: impl_type.clone(),
        signature: impl_sig,
        file_path: file_path.to_string(),
        line_number,
        doc: None,
        visibility: Visibility::Public,
    });

    for impl_item in &item.items {
        if let ImplItem::Fn(method) = impl_item {
            let visibility = convert_visibility(&method.vis);
            let method_doc = extract_doc_comment(&method.attrs);
            let method_line = get_line_number(&method.sig.fn_token.span);
            let method_sig =
                format_impl_method_signature(method, &impl_type, trait_name.as_deref());

            elements.push(CodeElement {
                language: Language::Rust,
                kind: ElementKind::Function,
                name: method.sig.ident.to_string(),
                signature: method_sig,
                file_path: file_path.to_string(),
                line_number: method_line,
                doc: method_doc,
                visibility,
            });
        }
    }
}

fn extract_const(item: &ItemConst, file_path: &str) -> Option<CodeElement> {
    let visibility = convert_visibility(&item.vis);
    let name = item.ident.to_string();
    let doc = extract_doc_comment(&item.attrs);
    let line_number = get_line_number(&item.const_token.span);

    let type_str = format_type(&item.ty);
    let signature = format!("const {name}: {type_str}");

    Some(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Const,
        name,
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility,
    })
}

fn extract_static(item: &ItemStatic, file_path: &str) -> Option<CodeElement> {
    let visibility = convert_visibility(&item.vis);
    let name = item.ident.to_string();
    let doc = extract_doc_comment(&item.attrs);
    let line_number = get_line_number(&item.static_token.span);

    let mutability = match item.mutability {
        syn::StaticMutability::Mut(_) => "mut ",
        _ => "",
    };
    let type_str = format_type(&item.ty);
    let signature = format!("static {mutability}{name}: {type_str}");

    Some(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Static,
        name,
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility,
    })
}

fn extract_type_alias(item: &ItemType, file_path: &str) -> Option<CodeElement> {
    let visibility = convert_visibility(&item.vis);
    let name = item.ident.to_string();
    let doc = extract_doc_comment(&item.attrs);
    let line_number = get_line_number(&item.type_token.span);

    let generics = format_generics(&item.generics);
    let type_str = format_type(&item.ty);
    let signature = format!("type {name}{generics} = {type_str}");

    Some(CodeElement {
        language: Language::Rust,
        kind: ElementKind::TypeAlias,
        name,
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility,
    })
}

fn extract_trait_assoc_type(
    item: &TraitItemType,
    trait_name: &str,
    file_path: &str,
) -> Option<CodeElement> {
    let name = item.ident.to_string();
    let doc = extract_doc_comment(&item.attrs);
    let line_number = get_line_number(&item.type_token.span);

    let bounds = if item.bounds.is_empty() {
        String::new()
    } else {
        let bounds_str = format_type_param_bounds(&item.bounds);
        format!(": {bounds_str}")
    };

    let default = item
        .default
        .as_ref()
        .map(|(_, ty)| format!(" = {}", format_type(ty)))
        .unwrap_or_default();

    let signature = format!("{trait_name}::type {name}{bounds}{default}");

    Some(CodeElement {
        language: Language::Rust,
        kind: ElementKind::TypeAlias,
        name,
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility: Visibility::Public,
    })
}

fn extract_trait_assoc_const(
    item: &TraitItemConst,
    trait_name: &str,
    file_path: &str,
) -> Option<CodeElement> {
    let name = item.ident.to_string();
    let doc = extract_doc_comment(&item.attrs);
    let line_number = get_line_number(&item.const_token.span);

    let type_str = format_type(&item.ty);
    let signature = format!("{trait_name}::const {name}: {type_str}");

    Some(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Const,
        name,
        signature,
        file_path: file_path.to_string(),
        line_number,
        doc,
        visibility: Visibility::Public,
    })
}

fn extract_module(item: &ItemMod, file_path: &str) -> Option<CodeElement> {
    let visibility = convert_visibility(&item.vis);
    let name = item.ident.to_string();
    let doc = extract_doc_comment(&item.attrs);
    let line_number = get_line_number(&item.mod_token.span);

    let signature = format!("mod {name}");

    Some(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Module,
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

fn format_function_signature(item: &ItemFn) -> String {
    let vis = format_visibility(&item.vis);
    let asyncness = if item.sig.asyncness.is_some() {
        "async "
    } else {
        ""
    };
    let unsafety = if item.sig.unsafety.is_some() {
        "unsafe "
    } else {
        ""
    };
    let name = &item.sig.ident;
    let generics = format_generics(&item.sig.generics);
    let params = format_fn_params(&item.sig.inputs);
    let ret = format_return_type(&item.sig.output);
    let where_clause = format_generics_where(&item.sig.generics);

    format!("{vis}{asyncness}{unsafety}fn {name}{generics}{params}{ret}{where_clause}")
}

fn format_struct_signature(item: &ItemStruct) -> String {
    let vis = format_visibility(&item.vis);
    let name = &item.ident;
    let generics = format_generics(&item.generics);

    let fields = match &item.fields {
        Fields::Named(fields) => {
            let field_strs: Vec<String> = fields
                .named
                .iter()
                .map(|f| {
                    let field_vis = format_visibility(&f.vis);
                    let field_name = f
                        .ident
                        .as_ref()
                        .map(std::string::ToString::to_string)
                        .unwrap_or_default();
                    let field_type = format_type(&f.ty);
                    format!("{field_vis}{field_name}: {field_type}")
                })
                .collect();
            format!(" {{ {} }}", field_strs.join(", "))
        }
        Fields::Unnamed(fields) => {
            let field_strs: Vec<String> = fields
                .unnamed
                .iter()
                .map(|f| {
                    let field_vis = format_visibility(&f.vis);
                    let field_type = format_type(&f.ty);
                    format!("{field_vis}{field_type}")
                })
                .collect();
            format!("({})", field_strs.join(", "))
        }
        Fields::Unit => String::new(),
    };

    format!("{vis}struct {name}{generics}{fields}")
}

fn format_enum_signature(item: &ItemEnum) -> String {
    let vis = format_visibility(&item.vis);
    let name = &item.ident;
    let generics = format_generics(&item.generics);

    let variants: Vec<String> = item
        .variants
        .iter()
        .map(|v| {
            let variant_name = &v.ident;
            match &v.fields {
                Fields::Named(fields) => {
                    let field_strs: Vec<String> = fields
                        .named
                        .iter()
                        .map(|f| {
                            let field_name = f
                                .ident
                                .as_ref()
                                .map(std::string::ToString::to_string)
                                .unwrap_or_default();
                            let field_type = format_type(&f.ty);
                            format!("{field_name}: {field_type}")
                        })
                        .collect();
                    format!("{} {{ {} }}", variant_name, field_strs.join(", "))
                }
                Fields::Unnamed(fields) => {
                    let field_strs: Vec<String> =
                        fields.unnamed.iter().map(|f| format_type(&f.ty)).collect();
                    format!("{}({})", variant_name, field_strs.join(", "))
                }
                Fields::Unit => variant_name.to_string(),
            }
        })
        .collect();

    format!(
        "{}enum {}{} {{ {} }}",
        vis,
        name,
        generics,
        variants.join(", ")
    )
}

fn format_trait_signature(item: &ItemTrait) -> String {
    let vis = format_visibility(&item.vis);
    let unsafety = if item.unsafety.is_some() {
        "unsafe "
    } else {
        ""
    };
    let name = &item.ident;
    let generics = format_generics(&item.generics);

    format!("{vis}{unsafety}trait {name}{generics}")
}

fn format_trait_method_signature(method: &syn::TraitItemFn, trait_name: &str) -> String {
    let asyncness = if method.sig.asyncness.is_some() {
        "async "
    } else {
        ""
    };
    let name = &method.sig.ident;
    let generics = format_generics(&method.sig.generics);
    let params = format_fn_params(&method.sig.inputs);
    let ret = format_return_type(&method.sig.output);
    let where_clause = format_generics_where(&method.sig.generics);

    format!("{trait_name}::{asyncness}fn {name}{generics}{params}{ret}{where_clause}")
}

fn format_impl_method_signature(
    method: &syn::ImplItemFn,
    impl_type: &str,
    trait_name: Option<&str>,
) -> String {
    let vis = format_visibility(&method.vis);
    let asyncness = if method.sig.asyncness.is_some() {
        "async "
    } else {
        ""
    };
    let name = &method.sig.ident;
    let generics = format_generics(&method.sig.generics);
    let params = format_fn_params(&method.sig.inputs);
    let ret = format_return_type(&method.sig.output);
    let where_clause = format_generics_where(&method.sig.generics);

    let prefix = if let Some(trait_n) = trait_name {
        format!("<{impl_type} as {trait_n}>::")
    } else {
        format!("{impl_type}::")
    };

    format!("{prefix}{vis}{asyncness}fn {name}{generics}{params}{ret}{where_clause}")
}

// ============================================================================
// Helper functions
// ============================================================================

fn extract_doc_comment(attrs: &[Attribute]) -> Option<String> {
    let docs: Vec<String> = attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc")
                && let syn::Meta::NameValue(nv) = &attr.meta
                && let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
            {
                return Some(s.value().trim().to_string());
            }
            None
        })
        .collect();

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

fn get_line_number(span: &::proc_macro2::Span) -> usize {
    span.start().line
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
    SynError { path: String, message: String },
}

#[cfg(test)]
#[path = "rust_parser_test.rs"]
mod rust_parser_test;
