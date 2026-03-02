//! Tests for aligned buffer module

use super::*;

#[test]
fn test_aligned_allocation() {
    let size = 65536;
    let align = 4096;

    let buf = AlignedBuffer::new(size, align);

    assert_eq!(buf.len(), size);
    assert!(buf.is_aligned());
    assert_eq!(buf.as_ptr() as usize % align, 0);
}

#[test]
fn test_fill_zero() {
    let mut buf = AlignedBuffer::new(1024, 512);
    buf.fill(0xAB); // Fill with something first
    buf.zero();

    assert!(buf.as_slice().iter().all(|&x| x == 0));
}

#[test]
fn test_fill_ones() {
    let mut buf = AlignedBuffer::new(1024, 512);
    buf.ones();

    assert!(buf.as_slice().iter().all(|&x| x == 0xFF));
}

#[test]
fn test_fill_pattern() {
    let mut buf = AlignedBuffer::new(1024, 512);

    buf.fill(0x33);
    assert!(buf.as_slice().iter().all(|&x| x == 0x33));

    buf.fill(0xAA);
    assert!(buf.as_slice().iter().all(|&x| x == 0xAA));
}

#[test]
fn test_as_mut_slice() {
    let buf = AlignedBuffer::new(1024, 512);
    let slice = buf.as_mut_slice();

    // Write to specific positions
    slice[0] = 0x12;
    slice[1023] = 0x34;

    assert_eq!(buf.as_slice()[0], 0x12);
    assert_eq!(buf.as_slice()[1023], 0x34);
}

#[test]
fn test_default_alignment() {
    let buf = AlignedBuffer::with_size(8192);
    assert_eq!(buf.alignment(), 4096);
    assert!(buf.is_aligned());
}

#[test]
#[should_panic(expected = "Alignment must be power of 2")]
fn test_invalid_alignment() {
    let _ = AlignedBuffer::new(1024, 1000); // 1000 is not power of 2
}

#[test]
#[should_panic(expected = "Size must be greater than 0")]
fn test_zero_size() {
    let _ = AlignedBuffer::new(0, 512);
}

#[test]
fn test_various_alignments() {
    for align in [512, 1024, 2048, 4096, 8192] {
        let buf = AlignedBuffer::new(align * 4, align);
        assert!(buf.is_aligned());
        assert_eq!(buf.alignment(), align);
    }
}
