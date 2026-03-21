//! Tests for stage pattern generators module

use super::*;

const TEST_SIZE: u64 = 10245; // Odd size to test partial blocks
const TEST_BLOCK: usize = 256;

#[test]
fn test_stage_display() {
    assert_eq!(format!("{}", Stage::zero()), "fill with 0x00");
    assert_eq!(format!("{}", Stage::one()), "fill with 0xFF");
    assert_eq!(format!("{}", Stage::constant(0xAB)), "fill with 0xAB");
    assert_eq!(format!("{}", Stage::random()), "random fill");
}

#[test]
fn test_zero_fill() {
    let stage = Stage::zero();
    let data = collect_stream(&stage, TEST_SIZE, TEST_BLOCK);

    assert_eq!(data.len(), TEST_SIZE as usize);
    assert!(data.iter().all(|&x| x == 0x00));
}

#[test]
fn test_ones_fill() {
    let stage = Stage::one();
    let data = collect_stream(&stage, TEST_SIZE, TEST_BLOCK);

    assert_eq!(data.len(), TEST_SIZE as usize);
    assert!(data.iter().all(|&x| x == 0xFF));
}

#[test]
fn test_constant_fill() {
    let stage = Stage::constant(0x42);
    let data = collect_stream(&stage, TEST_SIZE, TEST_BLOCK);

    assert_eq!(data.len(), TEST_SIZE as usize);
    assert!(data.iter().all(|&x| x == 0x42));
}

#[test]
fn test_random_fill_produces_data() {
    let stage = Stage::random_with_seed([13; 32]);
    let data = collect_stream(&stage, TEST_SIZE, TEST_BLOCK);

    assert_eq!(data.len(), TEST_SIZE as usize);

    // Should not be all zeros or all ones
    assert!(!data.iter().all(|&x| x == 0x00));
    assert!(!data.iter().all(|&x| x == 0xFF));
}

#[test]
fn test_random_fill_is_reproducible() {
    let seed = [42u8; 32];

    let stage1 = Stage::random_with_seed(seed);
    let data1 = collect_stream(&stage1, TEST_SIZE, TEST_BLOCK);

    let stage2 = Stage::random_with_seed(seed);
    let data2 = collect_stream(&stage2, TEST_SIZE, TEST_BLOCK);

    assert_eq!(data1, data2, "Same seed should produce same data");
}

#[test]
fn test_different_seeds_produce_different_data() {
    let stage1 = Stage::random_with_seed([1; 32]);
    let data1 = collect_stream(&stage1, TEST_SIZE, TEST_BLOCK);

    let stage2 = Stage::random_with_seed([2; 32]);
    let data2 = collect_stream(&stage2, TEST_SIZE, TEST_BLOCK);

    assert_ne!(
        data1, data2,
        "Different seeds should produce different data"
    );
}

#[test]
fn test_stream_progress() {
    let stage = Stage::zero();
    let mut stream = stage.stream(1024, 128, 0);

    assert_eq!(stream.progress(), 0.0);

    stream.advance();
    assert!((stream.progress() - 0.125).abs() < 0.001); // 128/1024 = 0.125

    // Consume the rest
    while stream.advance() {}

    assert_eq!(stream.progress(), 1.0);
}

#[test]
fn test_stream_resume_from_offset() {
    let seed = [99u8; 32];
    let total_size = 1024u64;
    let block_size = 256;

    // Generate full data
    let stage = Stage::random_with_seed(seed);
    let full_data = collect_stream(&stage, total_size, block_size);

    // Generate from offset
    let offset = 512u64;
    let stage2 = Stage::random_with_seed(seed);
    let partial_data = collect_stream_from(&stage2, total_size, block_size, offset);

    // The partial data should match the second half of full data
    assert_eq!(partial_data.len(), (total_size - offset) as usize);
    assert_eq!(&full_data[offset as usize..], &partial_data[..]);
}

#[test]
fn test_stream_reset() {
    let seed = [77u8; 32];
    let stage = Stage::random_with_seed(seed);

    let mut stream = stage.stream(1024, 128, 0);

    // Collect first 512 bytes
    let mut first_512 = Vec::new();
    while stream.position() < 512 {
        if stream.advance() {
            first_512.extend_from_slice(stream.get().unwrap());
        }
    }

    // Reset and collect again
    stream.reset_to(0);
    let mut second_512 = Vec::new();
    while stream.position() < 512 {
        if stream.advance() {
            second_512.extend_from_slice(stream.get().unwrap());
        }
    }

    assert_eq!(first_512, second_512, "Reset should reproduce same data");
}

#[test]
fn test_empty_stream() {
    let stage = Stage::zero();
    let mut stream = stage.stream(0, 256, 0);

    assert!(!stream.advance());
    assert!(stream.is_finished());
}

#[test]
fn test_stage_description() {
    assert_eq!(Stage::zero().description(), "zero fill");
    assert_eq!(Stage::one().description(), "ones fill");
    assert_eq!(Stage::constant(0x55).description(), "pattern fill");
    assert_eq!(Stage::random().description(), "random fill");
}

// Helper functions for tests

fn collect_stream(stage: &Stage, total_size: u64, block_size: usize) -> Vec<u8> {
    collect_stream_from(stage, total_size, block_size, 0)
}

fn collect_stream_from(
    stage: &Stage,
    total_size: u64,
    block_size: usize,
    start_from: u64,
) -> Vec<u8> {
    let mut stream = stage.stream(total_size, block_size, start_from);
    let mut result = Vec::new();

    while stream.advance() {
        if let Some(chunk) = stream.get() {
            result.extend_from_slice(chunk);
        }
    }

    result
}
