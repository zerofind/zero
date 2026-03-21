//! Formatting helpers for Rust AST nodes
//!
//! Converts syn AST nodes into human-readable string representations.

use syn::{FnArg, GenericParam, Generics, Pat, ReturnType, Type, Visibility as SynVisibility};

use super::super::element::Visibility;

/// Format a syn Visibility as a string prefix
pub fn format_visibility(vis: &SynVisibility) -> String {
    match vis {
        SynVisibility::Public(_) => "pub ".to_string(),
        SynVisibility::Restricted(r) => {
            let path = &r.path;
            if path.is_ident("crate") {
                "pub(crate) ".to_string()
            } else if path.is_ident("super") {
                "pub(super) ".to_string()
            } else {
                let path_str = path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                format!("pub({path_str}) ")
            }
        }
        SynVisibility::Inherited => String::new(),
    }
}

/// Convert syn Visibility to our Visibility enum
pub fn convert_visibility(vis: &SynVisibility) -> Visibility {
    match vis {
        SynVisibility::Public(_) => Visibility::Public,
        SynVisibility::Restricted(r) => {
            let path = &r.path;
            if path.is_ident("crate") {
                Visibility::PublicCrate
            } else if path.is_ident("super") {
                Visibility::PublicSuper
            } else {
                Visibility::Private
            }
        }
        SynVisibility::Inherited => Visibility::Private,
    }
}

/// Format generic parameters (including trait bounds, but NOT where clause)
pub fn format_generics(generics: &Generics) -> String {
    if generics.params.is_empty() {
        return String::new();
    }

    let params: Vec<String> = generics
        .params
        .iter()
        .map(|p| match p {
            GenericParam::Type(t) => {
                let name = t.ident.to_string();
                if t.bounds.is_empty() {
                    name
                } else {
                    let bounds = format_type_param_bounds(&t.bounds);
                    format!("{name}: {bounds}")
                }
            }
            GenericParam::Lifetime(l) => {
                let name = format!("'{}", l.lifetime.ident);
                if l.bounds.is_empty() {
                    name
                } else {
                    let bounds: Vec<String> =
                        l.bounds.iter().map(|b| format!("'{}", b.ident)).collect();
                    format!("{}: {}", name, bounds.join(" + "))
                }
            }
            GenericParam::Const(c) => format!("const {}: {}", c.ident, format_type(&c.ty)),
        })
        .collect();

    format!("<{}>", params.join(", "))
}

/// Format the where clause from generics (if present)
pub fn format_generics_where(generics: &Generics) -> String {
    format_where_clause(&generics.where_clause)
}

/// Format a where clause
pub fn format_where_clause(where_clause: &Option<syn::WhereClause>) -> String {
    match where_clause {
        None => String::new(),
        Some(wc) if wc.predicates.is_empty() => String::new(),
        Some(wc) => {
            let predicates: Vec<String> = wc
                .predicates
                .iter()
                .map(|pred| match pred {
                    syn::WherePredicate::Type(pt) => {
                        let ty = format_type(&pt.bounded_ty);
                        let bounds = format_type_param_bounds(&pt.bounds);
                        format!("{ty}: {bounds}")
                    }
                    syn::WherePredicate::Lifetime(pl) => {
                        let lifetime = format!("'{}", pl.lifetime.ident);
                        let bounds: Vec<String> =
                            pl.bounds.iter().map(|b| format!("'{}", b.ident)).collect();
                        format!("{}: {}", lifetime, bounds.join(" + "))
                    }
                    _ => "_".to_string(),
                })
                .collect();
            format!(" where {}", predicates.join(", "))
        }
    }
}

/// Format type parameter bounds (e.g., `Clone + Debug + 'a`)
pub fn format_type_param_bounds(
    bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::token::Plus>,
) -> String {
    bounds
        .iter()
        .map(|bound| match bound {
            syn::TypeParamBound::Trait(tb) => format_trait_bound(tb),
            syn::TypeParamBound::Lifetime(lt) => format!("'{}", lt.ident),
            _ => "_".to_string(),
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Format a trait bound (e.g., `Iterator<Item = u32>` or `?Sized`)
fn format_trait_bound(tb: &syn::TraitBound) -> String {
    let modifier = match tb.modifier {
        syn::TraitBoundModifier::None => "",
        syn::TraitBoundModifier::Maybe(_) => "?",
    };

    let path: Vec<String> = tb
        .path
        .segments
        .iter()
        .map(|seg| {
            let ident = seg.ident.to_string();
            match &seg.arguments {
                syn::PathArguments::None => ident,
                syn::PathArguments::AngleBracketed(args) => {
                    let args_str: Vec<String> = args
                        .args
                        .iter()
                        .map(|arg| match arg {
                            syn::GenericArgument::Type(t) => format_type(t),
                            syn::GenericArgument::Lifetime(l) => format!("'{}", l.ident),
                            syn::GenericArgument::AssocType(at) => {
                                let ty = format_type(&at.ty);
                                format!("{} = {}", at.ident, ty)
                            }
                            syn::GenericArgument::Constraint(c) => {
                                let bounds = format_type_param_bounds(&c.bounds);
                                format!("{}: {}", c.ident, bounds)
                            }
                            _ => "_".to_string(),
                        })
                        .collect();
                    if args_str.is_empty() {
                        ident
                    } else {
                        format!("{}<{}>", ident, args_str.join(", "))
                    }
                }
                syn::PathArguments::Parenthesized(args) => {
                    let inputs: Vec<String> = args.inputs.iter().map(format_type).collect();
                    let output = match &args.output {
                        ReturnType::Default => String::new(),
                        ReturnType::Type(_, t) => format!(" -> {}", format_type(t)),
                    };
                    format!("{}({}){}", ident, inputs.join(", "), output)
                }
            }
        })
        .collect();

    format!("{}{}", modifier, path.join("::"))
}

/// Format function parameters
pub fn format_fn_params(inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>) -> String {
    let params: Vec<String> = inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Receiver(r) => {
                let ref_tok = if r.reference.is_some() { "&" } else { "" };
                let mutability = if r.mutability.is_some() { "mut " } else { "" };
                format!("{ref_tok}{mutability}self")
            }
            FnArg::Typed(t) => {
                let name = match t.pat.as_ref() {
                    Pat::Ident(i) => i.ident.to_string(),
                    _ => "_".to_string(),
                };
                let ty = format_type(&t.ty);
                format!("{name}: {ty}")
            }
        })
        .collect();

    format!("({})", params.join(", "))
}

/// Format return type
pub fn format_return_type(ret: &ReturnType) -> String {
    match ret {
        ReturnType::Default => String::new(),
        ReturnType::Type(_, ty) => format!(" -> {}", format_type(ty)),
    }
}

/// Format a Type node as a string
pub fn format_type(ty: &Type) -> String {
    use syn::Type::{Array, ImplTrait, Never, Path, Ptr, Reference, Slice, Tuple};
    match ty {
        Path(p) => p
            .path
            .segments
            .iter()
            .map(|seg| {
                let ident = seg.ident.to_string();
                match &seg.arguments {
                    syn::PathArguments::None => ident,
                    syn::PathArguments::AngleBracketed(args) => {
                        let args_str: Vec<String> = args
                            .args
                            .iter()
                            .map(|arg| match arg {
                                syn::GenericArgument::Type(t) => format_type(t),
                                syn::GenericArgument::Lifetime(l) => {
                                    format!("'{}", l.ident)
                                }
                                syn::GenericArgument::AssocType(at) => {
                                    let ty = format_type(&at.ty);
                                    format!("{} = {}", at.ident, ty)
                                }
                                syn::GenericArgument::Constraint(c) => {
                                    let bounds = format_type_param_bounds(&c.bounds);
                                    format!("{}: {}", c.ident, bounds)
                                }
                                _ => "_".to_string(),
                            })
                            .collect();
                        format!("{}<{}>", ident, args_str.join(", "))
                    }
                    syn::PathArguments::Parenthesized(args) => {
                        let inputs: Vec<String> = args.inputs.iter().map(format_type).collect();
                        let output = match &args.output {
                            ReturnType::Default => String::new(),
                            ReturnType::Type(_, t) => format!(" -> {}", format_type(t)),
                        };
                        format!("{}({}){}", ident, inputs.join(", "), output)
                    }
                }
            })
            .collect::<Vec<_>>()
            .join("::"),
        Reference(r) => {
            let lifetime = r
                .lifetime
                .as_ref()
                .map(|l| format!("'{} ", l.ident))
                .unwrap_or_default();
            let mutability = if r.mutability.is_some() { "mut " } else { "" };
            format!("&{}{}{}", lifetime, mutability, format_type(&r.elem))
        }
        Ptr(p) => {
            let mutability = if p.mutability.is_some() {
                "mut "
            } else {
                "const "
            };
            format!("*{}{}", mutability, format_type(&p.elem))
        }
        Slice(s) => format!("[{}]", format_type(&s.elem)),
        Array(a) => {
            let len = match &a.len {
                syn::Expr::Lit(lit) => match &lit.lit {
                    syn::Lit::Int(i) => i.to_string(),
                    _ => "_".to_string(),
                },
                _ => "_".to_string(),
            };
            format!("[{}; {}]", format_type(&a.elem), len)
        }
        Tuple(t) => {
            let elems: Vec<String> = t.elems.iter().map(format_type).collect();
            format!("({})", elems.join(", "))
        }
        Never(_) => "!".to_string(),
        ImplTrait(i) => {
            let bounds = format_type_param_bounds(&i.bounds);
            format!("impl {bounds}")
        }
        _ => "_".to_string(),
    }
}

#[cfg(test)]
#[path = "rust_format_test.rs"]
mod rust_format_test;
