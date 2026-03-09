//! Tests for Rust AST formatting helpers (ported from Compass)

use super::*;
use syn::parse_quote;

use super::super::super::element::Visibility;

#[test]
fn test_format_simple_type() {
    let ty: syn::Type = parse_quote!(String);
    assert_eq!(format_type(&ty), "String");
}

#[test]
fn test_format_reference_type() {
    let ty: syn::Type = parse_quote!(&str);
    assert_eq!(format_type(&ty), "&str");
}

#[test]
fn test_format_mutable_reference() {
    let ty: syn::Type = parse_quote!(&mut Vec<u8>);
    assert_eq!(format_type(&ty), "&mut Vec<u8>");
}

#[test]
fn test_format_generic_type() {
    let ty: syn::Type = parse_quote!(Option<String>);
    assert_eq!(format_type(&ty), "Option<String>");
}

#[test]
fn test_format_nested_generic() {
    let ty: syn::Type = parse_quote!(Result<Vec<u8>, Error>);
    assert_eq!(format_type(&ty), "Result<Vec<u8>, Error>");
}

#[test]
fn test_format_tuple() {
    let ty: syn::Type = parse_quote!((u32, String));
    assert_eq!(format_type(&ty), "(u32, String)");
}

#[test]
fn test_format_slice() {
    let ty: syn::Type = parse_quote!([u8]);
    assert_eq!(format_type(&ty), "[u8]");
}

#[test]
fn test_format_array() {
    let ty: syn::Type = parse_quote!([u8; 32]);
    assert_eq!(format_type(&ty), "[u8; 32]");
}

#[test]
fn test_format_never() {
    let ty: syn::Type = parse_quote!(!);
    assert_eq!(format_type(&ty), "!");
}

#[test]
fn test_format_visibility_public() {
    let vis: syn::Visibility = parse_quote!(pub);
    assert_eq!(format_visibility(&vis), "pub ");
}

#[test]
fn test_format_visibility_private() {
    let vis: syn::Visibility = parse_quote!();
    assert_eq!(format_visibility(&vis), "");
}

#[test]
fn test_format_visibility_crate() {
    let vis: syn::Visibility = parse_quote!(pub(crate));
    assert_eq!(format_visibility(&vis), "pub(crate) ");
}

#[test]
fn test_convert_visibility() {
    let pub_vis: syn::Visibility = parse_quote!(pub);
    assert_eq!(convert_visibility(&pub_vis), Visibility::Public);

    let priv_vis: syn::Visibility = parse_quote!();
    assert_eq!(convert_visibility(&priv_vis), Visibility::Private);

    let crate_vis: syn::Visibility = parse_quote!(pub(crate));
    assert_eq!(convert_visibility(&crate_vis), Visibility::PublicCrate);
}

#[test]
fn test_format_generics_empty() {
    let generics: syn::Generics = parse_quote!();
    assert_eq!(format_generics(&generics), "");
}

#[test]
fn test_format_generics_single() {
    let generics: syn::Generics = parse_quote!(<T>);
    assert_eq!(format_generics(&generics), "<T>");
}

#[test]
fn test_format_generics_multiple() {
    let generics: syn::Generics = parse_quote!(<T, U>);
    assert_eq!(format_generics(&generics), "<T, U>");
}

#[test]
fn test_format_generics_with_lifetime() {
    let generics: syn::Generics = parse_quote!(<'a, T>);
    assert_eq!(format_generics(&generics), "<'a, T>");
}
