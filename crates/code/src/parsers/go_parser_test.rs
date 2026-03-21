//! Tests for the Go source parser

use super::*;
use crate::element::{ElementKind, Visibility};

#[test]
fn test_parse_simple_function() {
    let source = r#"
package main

func Hello(name string) string {
    return "Hello, " + name
}
"#;

    let elements = parse_source(source, "main.go").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].kind, ElementKind::Function);
    assert_eq!(elements[0].name, "Hello");
    assert_eq!(elements[0].visibility, Visibility::Public);
    assert!(elements[0].signature.contains("func Hello"));
}

#[test]
fn test_parse_private_function() {
    let source = r#"
package main

func helper() {}
"#;

    let elements = parse_source(source, "main.go").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].name, "helper");
    assert_eq!(elements[0].visibility, Visibility::Private);
}

#[test]
fn test_parse_struct() {
    let source = r#"
package main

type Config struct {
    Name    string
    Port    int
    Verbose bool
}
"#;

    let elements = parse_source(source, "main.go").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].kind, ElementKind::Struct);
    assert_eq!(elements[0].name, "Config");
    assert!(elements[0].signature.contains("type Config struct"));
    assert!(elements[0].signature.contains("Name"));
}

#[test]
fn test_parse_interface() {
    let source = r#"
package main

type Reader interface {
    Read(p []byte) (n int, err error)
}
"#;

    let elements = parse_source(source, "main.go").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].kind, ElementKind::Interface);
    assert_eq!(elements[0].name, "Reader");
    assert!(elements[0].signature.contains("type Reader interface"));
    assert!(elements[0].signature.contains("Read"));
}

#[test]
fn test_parse_method() {
    let source = r#"
package main

type Server struct {}

func (s *Server) Start(port int) error {
    return nil
}
"#;

    let elements = parse_source(source, "main.go").unwrap();

    let method = elements
        .iter()
        .find(|e| e.kind == ElementKind::Method)
        .unwrap();
    assert_eq!(method.name, "Start");
    assert!(method.signature.contains("func (*Server) Start"));
}

#[test]
fn test_parse_const() {
    let source = r#"
package main

const MaxSize int = 1024
"#;

    let elements = parse_source(source, "main.go").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].kind, ElementKind::Const);
    assert_eq!(elements[0].name, "MaxSize");
    assert!(elements[0].signature.contains("const MaxSize"));
}

#[test]
fn test_parse_var() {
    let source = r#"
package main

var DefaultConfig Config
"#;

    let elements = parse_source(source, "main.go").unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].kind, ElementKind::Static);
    assert_eq!(elements[0].name, "DefaultConfig");
}

#[test]
fn test_parse_doc_comment() {
    let source = r#"
package main

// Greet returns a greeting message.
func Greet(name string) string {
    return "hi"
}
"#;

    let elements = parse_source(source, "main.go").unwrap();

    assert_eq!(elements.len(), 1);
    assert!(elements[0].doc.is_some());
    assert!(elements[0].doc.as_ref().unwrap().contains("greeting"));
}

#[test]
fn test_parse_function_with_return() {
    let source = r#"
package main

func Add(a, b int) int {
    return a + b
}
"#;

    let elements = parse_source(source, "main.go").unwrap();

    assert_eq!(elements.len(), 1);
    assert!(elements[0].signature.contains("int"));
}

#[test]
fn test_parse_empty_file() {
    let source = "package main\n";
    let elements = parse_source(source, "main.go").unwrap();
    assert!(elements.is_empty());
}

#[test]
fn test_parse_complex_file() {
    let source = r#"
package server

const Version = "1.0"

type Server struct {
    Port int
    Host string
}

type Handler interface {
    Handle(req Request) Response
}

func NewServer(port int) *Server {
    return &Server{Port: port}
}

func (s *Server) Listen() error {
    return nil
}
"#;

    let elements = parse_source(source, "server.go").unwrap();

    let kinds: Vec<ElementKind> = elements.iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&ElementKind::Const));
    assert!(kinds.contains(&ElementKind::Struct));
    assert!(kinds.contains(&ElementKind::Interface));
    assert!(kinds.contains(&ElementKind::Function));
    assert!(kinds.contains(&ElementKind::Method));
    assert!(elements.len() >= 5);
}

#[test]
fn test_go_visibility_convention() {
    let source = r#"
package main

func Exported() {}
func unexported() {}

type PublicType struct {}
type privateType struct {}
"#;

    let elements = parse_source(source, "main.go").unwrap();

    let exported_fn = elements.iter().find(|e| e.name == "Exported").unwrap();
    assert_eq!(exported_fn.visibility, Visibility::Public);

    let unexported_fn = elements.iter().find(|e| e.name == "unexported").unwrap();
    assert_eq!(unexported_fn.visibility, Visibility::Private);

    let pub_type = elements.iter().find(|e| e.name == "PublicType").unwrap();
    assert_eq!(pub_type.visibility, Visibility::Public);

    let priv_type = elements.iter().find(|e| e.name == "privateType").unwrap();
    assert_eq!(priv_type.visibility, Visibility::Private);
}
