//! Tests for hash algorithm definitions

use super::*;

#[test]
fn test_digest_sizes() {
    assert_eq!(HashAlgorithm::Blake3.digest_size(), 32);
    assert_eq!(HashAlgorithm::Xxh3.digest_size(), 16);
}

#[test]
fn test_from_str() {
    assert_eq!(
        HashAlgorithm::try_from("blake3").unwrap(),
        HashAlgorithm::Blake3
    );
    assert_eq!(
        HashAlgorithm::try_from("BLAKE3").unwrap(),
        HashAlgorithm::Blake3
    );
    assert_eq!(
        HashAlgorithm::try_from("xxh3").unwrap(),
        HashAlgorithm::Xxh3
    );
    assert!(HashAlgorithm::try_from("md5").is_err());
}

#[test]
fn test_from_str_case_insensitive() {
    assert_eq!(
        HashAlgorithm::try_from("Blake3").unwrap(),
        HashAlgorithm::Blake3
    );
    assert_eq!(
        HashAlgorithm::try_from("XXH3").unwrap(),
        HashAlgorithm::Xxh3
    );
    assert_eq!(
        HashAlgorithm::try_from("Xxh3").unwrap(),
        HashAlgorithm::Xxh3
    );
}

#[test]
fn test_from_str_invalid() {
    let result = HashAlgorithm::try_from("sha256");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown hash algorithm"));

    let result2 = HashAlgorithm::try_from("md5");
    assert!(result2.is_err());
}

#[test]
fn test_default_is_xxh3() {
    assert_eq!(HashAlgorithm::default(), HashAlgorithm::Xxh3);
}

#[test]
fn test_cryptographic() {
    assert!(HashAlgorithm::Blake3.is_cryptographic());
    assert!(!HashAlgorithm::Xxh3.is_cryptographic());
}

#[test]
fn test_algorithm_names() {
    assert_eq!(HashAlgorithm::Blake3.name(), "blake3");
    assert_eq!(HashAlgorithm::Xxh3.name(), "xxh3");
}

#[test]
fn test_display() {
    assert_eq!(format!("{}", HashAlgorithm::Blake3), "blake3");
    assert_eq!(format!("{}", HashAlgorithm::Xxh3), "xxh3");
}

#[test]
fn test_clone_and_copy() {
    let algo = HashAlgorithm::Blake3;
    let cloned = algo;
    let copied = algo;

    assert_eq!(algo, cloned);
    assert_eq!(algo, copied);
}

#[test]
fn test_debug() {
    let debug_str = format!("{:?}", HashAlgorithm::Blake3);
    assert!(debug_str.contains("Blake3"));

    let debug_str2 = format!("{:?}", HashAlgorithm::Xxh3);
    assert!(debug_str2.contains("Xxh3"));
}

#[test]
fn test_equality() {
    assert_eq!(HashAlgorithm::Blake3, HashAlgorithm::Blake3);
    assert_eq!(HashAlgorithm::Xxh3, HashAlgorithm::Xxh3);
    assert_ne!(HashAlgorithm::Blake3, HashAlgorithm::Xxh3);
}
