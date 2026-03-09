//! Tests for the Rust source parser (ported from Compass)

use super::super::super::element::{ElementKind, Visibility};
use super::*;

#[test]
fn test_parse_simple_function() {
    let source = r#"
        pub fn hello(name: &str) -> String {
            format!("Hello, {}!", name)
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].kind, ElementKind::Function);
    assert_eq!(elements[0].name, "hello");
    assert!(elements[0].signature.contains("pub fn hello"));
    assert!(elements[0].signature.contains("name:"));
    assert!(elements[0].signature.contains("str"));
    assert!(elements[0].signature.contains("-> String"));
    assert_eq!(elements[0].visibility, Visibility::Public);
}

#[test]
fn test_parse_private_function() {
    let source = r#"
        fn private_fn() {}
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].visibility, Visibility::Private);
}

#[test]
fn test_parse_struct() {
    let source = r#"
        pub struct User {
            pub name: String,
            pub age: u32,
            email: String,
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].kind, ElementKind::Struct);
    assert_eq!(elements[0].name, "User");
    assert!(elements[0].signature.contains("pub struct User"));
    assert!(elements[0].signature.contains("pub name: String"));
    assert!(elements[0].signature.contains("pub age: u32"));
}

#[test]
fn test_parse_tuple_struct() {
    let source = r#"
        pub struct Point(pub f64, pub f64);
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].kind, ElementKind::Struct);
    assert!(elements[0].signature.contains("Point"));
    assert!(elements[0].signature.contains("f64"));
}

#[test]
fn test_parse_enum() {
    let source = r#"
        pub enum Status {
            Active,
            Inactive,
            Pending(String),
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].kind, ElementKind::Enum);
    assert_eq!(elements[0].name, "Status");
    assert!(elements[0].signature.contains("Active"));
    assert!(elements[0].signature.contains("Inactive"));
    assert!(elements[0].signature.contains("Pending"));
}

#[test]
fn test_parse_trait() {
    let source = r#"
        pub trait Drawable {
            fn draw(&self);
            fn area(&self) -> f64;
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    // trait + 2 methods
    assert_eq!(elements.len(), 3);
    assert_eq!(elements[0].kind, ElementKind::Trait);
    assert_eq!(elements[0].name, "Drawable");
    assert_eq!(elements[1].kind, ElementKind::Function);
    assert_eq!(elements[1].name, "draw");
    assert_eq!(elements[2].kind, ElementKind::Function);
    assert_eq!(elements[2].name, "area");
}

#[test]
fn test_parse_impl_block() {
    let source = r#"
        struct Point { x: f64, y: f64 }

        impl Point {
            pub fn new(x: f64, y: f64) -> Self {
                Self { x, y }
            }

            pub fn distance(&self, other: &Point) -> f64 {
                0.0
            }
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    // struct + impl + 2 methods
    assert_eq!(elements.len(), 4);

    let impl_elem = elements
        .iter()
        .find(|e| e.kind == ElementKind::Impl)
        .unwrap();
    assert!(impl_elem.signature.contains("impl Point"));

    let new_fn = elements.iter().find(|e| e.name == "new").unwrap();
    assert!(new_fn.signature.contains("Point::"));
    assert!(new_fn.signature.contains("pub fn new"));
}

#[test]
fn test_parse_trait_impl() {
    let source = r#"
        struct Circle { radius: f64 }

        impl Default for Circle {
            fn default() -> Self {
                Self { radius: 1.0 }
            }
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    let impl_elem = elements
        .iter()
        .find(|e| e.kind == ElementKind::Impl)
        .unwrap();
    assert!(impl_elem.signature.contains("impl Default for Circle"));

    let default_fn = elements.iter().find(|e| e.name == "default").unwrap();
    assert!(default_fn.signature.contains("<Circle as Default>::"));
}

#[test]
fn test_parse_const() {
    let source = r#"
        pub const MAX_SIZE: usize = 1024;
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].kind, ElementKind::Const);
    assert_eq!(elements[0].name, "MAX_SIZE");
    assert!(elements[0].signature.contains("const MAX_SIZE: usize"));
}

#[test]
fn test_parse_static() {
    let source = r#"
        pub static mut COUNTER: u32 = 0;
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].kind, ElementKind::Static);
    assert!(elements[0].signature.contains("static mut COUNTER"));
}

#[test]
fn test_parse_type_alias() {
    let source = r#"
        pub type Result<T> = std::result::Result<T, Error>;
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].kind, ElementKind::TypeAlias);
    assert!(elements[0].signature.contains("type Result"));
}

#[test]
fn test_parse_doc_comment() {
    let source = r#"
        /// This is a documented function.
        /// It does important things.
        pub fn documented() {}
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert!(elements[0].doc.is_some());
    let doc = elements[0].doc.as_ref().unwrap();
    assert!(doc.contains("documented function"));
    assert!(doc.contains("important things"));
}

#[test]
fn test_parse_async_function() {
    let source = r#"
        pub async fn fetch_data(url: &str) -> Result<String, Error> {
            todo!()
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert!(elements[0].signature.contains("async"));
}

#[test]
fn test_parse_generic_function() {
    let source = r#"
        pub fn process<T: Clone>(item: T) -> T {
            item.clone()
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert!(elements[0].signature.contains("<T: Clone>"));
}

#[test]
fn test_parse_where_clause() {
    let source = r#"
        pub fn complex<T, U>(a: T, b: U) -> T
        where
            T: Clone + Send,
            U: std::fmt::Debug,
        {
            a.clone()
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert!(elements[0].signature.contains("where"));
    assert!(elements[0].signature.contains("T: Clone + Send"));
    assert!(elements[0].signature.contains("U:"));
    assert!(elements[0].signature.contains("Debug"));
}

#[test]
fn test_parse_multiple_trait_bounds() {
    let source = r#"
        pub fn multi_bound<T: Clone + Send + Sync>(item: T) -> T {
            item.clone()
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert!(elements[0].signature.contains("Clone"));
    assert!(elements[0].signature.contains("Send"));
    assert!(elements[0].signature.contains("Sync"));
}

#[test]
fn test_parse_trait_with_associated_type() {
    let source = r#"
        pub trait Iterator {
            type Item;
            fn next(&mut self) -> Option<Self::Item>;
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    // trait + associated type + method
    assert_eq!(elements.len(), 3);

    let assoc_type = elements
        .iter()
        .find(|e| e.kind == ElementKind::TypeAlias && e.name == "Item")
        .unwrap();
    assert!(assoc_type.signature.contains("Iterator::type Item"));
}

#[test]
fn test_parse_trait_with_associated_const() {
    let source = r#"
        pub trait Bounded {
            const MAX: usize;
            const MIN: usize;
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    // trait + 2 associated constants
    assert_eq!(elements.len(), 3);

    let assoc_const = elements
        .iter()
        .find(|e| e.kind == ElementKind::Const && e.name == "MAX")
        .unwrap();
    assert!(assoc_const.signature.contains("Bounded::const MAX: usize"));
}

#[test]
fn test_parse_associated_type_with_bounds() {
    let source = r#"
        pub trait Container {
            type Item: Clone + Send;
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    let assoc_type = elements
        .iter()
        .find(|e| e.kind == ElementKind::TypeAlias)
        .unwrap();
    assert!(assoc_type.signature.contains("Clone"));
    assert!(assoc_type.signature.contains("Send"));
}

#[test]
fn test_parse_impl_trait_return() {
    let source = r#"
        pub fn make_iter() -> impl Iterator<Item = u32> + Send {
            vec![1, 2, 3].into_iter()
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert!(elements[0].signature.contains("impl Iterator"));
    assert!(elements[0].signature.contains("Item = u32"));
    assert!(elements[0].signature.contains("Send"));
}

#[test]
fn test_parse_module() {
    let source = r#"
        pub mod utils;
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].kind, ElementKind::Module);
    assert_eq!(elements[0].name, "utils");
}

#[test]
fn test_parse_pub_crate_visibility() {
    let source = r#"
        pub(crate) fn internal() {}
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].visibility, Visibility::PublicCrate);
}

#[test]
fn test_parse_empty_file() {
    let source = "";
    let elements = parse_source(source, "test.rs").unwrap();
    assert!(elements.is_empty());
}

#[test]
fn test_parse_invalid_syntax() {
    let source = "pub fn broken( {}";
    let result = parse_source(source, "test.rs");
    assert!(result.is_err());
}

#[test]
fn test_parse_complex_file() {
    let source = r#"
        //! Module documentation

        pub const VERSION: &str = "1.0.0";

        /// A user in the system
        pub struct User {
            pub id: u64,
            pub name: String,
        }

        /// User-related errors
        pub enum UserError {
            NotFound,
            InvalidInput(String),
        }

        impl User {
            /// Create a new user
            pub fn new(id: u64, name: String) -> Self {
                Self { id, name }
            }
        }

        /// Repository trait
        pub trait Repository {
            fn save(&self, user: &User) -> Result<(), UserError>;
            fn find(&self, id: u64) -> Option<User>;
        }
    "#;

    let elements = parse_source(source, "test.rs").unwrap();

    // const + struct + enum + impl + impl::new + trait + trait::save + trait::find
    assert!(elements.len() >= 7);

    let kinds: Vec<ElementKind> = elements.iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&ElementKind::Const));
    assert!(kinds.contains(&ElementKind::Struct));
    assert!(kinds.contains(&ElementKind::Enum));
    assert!(kinds.contains(&ElementKind::Impl));
    assert!(kinds.contains(&ElementKind::Trait));
    assert!(kinds.contains(&ElementKind::Function));
}
