use std::hint::black_box;

use blake3::Hasher as Blake3Hasher;
use xxhash_rust::xxh3::Xxh3;

// -- fixtures -----------------------------------------------------------------

fn test_data(size: usize) -> Vec<u8> {
    // Deterministic pseudo-random data (not urandom — reproducible)
    let mut data = vec![0u8; size];
    let mut state: u64 = 0xDEAD_BEEF_CAFE_BABE;
    for chunk in data.chunks_mut(8) {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let bytes = state.to_le_bytes();
        let len = chunk.len().min(8);
        chunk[..len].copy_from_slice(&bytes[..len]);
    }
    data
}

const KB: usize = 1024;
const MB: usize = 1024 * 1024;

// -- xxh3 throughput ----------------------------------------------------------

#[divan::bench(args = [KB, 64 * KB, MB, 16 * MB])]
fn xxh3_throughput(bencher: divan::Bencher, size: usize) {
    let data = test_data(size);
    bencher
        .counter(divan::counter::BytesCount::new(size))
        .bench(|| {
            let mut hasher = Xxh3::new();
            hasher.update(black_box(&data));
            black_box(hasher.digest128());
        });
}

// -- blake3 throughput --------------------------------------------------------

#[divan::bench(args = [KB, 64 * KB, MB, 16 * MB])]
fn blake3_throughput(bencher: divan::Bencher, size: usize) {
    let data = test_data(size);
    bencher
        .counter(divan::counter::BytesCount::new(size))
        .bench(|| {
            let mut hasher = Blake3Hasher::new();
            hasher.update(black_box(&data));
            black_box(hasher.finalize());
        });
}

// -- chunked hash (simulates buffered I/O path) -------------------------------

const BUF_SIZE: usize = 128 * KB;

#[divan::bench(args = [MB, 16 * MB])]
fn xxh3_chunked(bencher: divan::Bencher, size: usize) {
    let data = test_data(size);
    bencher
        .counter(divan::counter::BytesCount::new(size))
        .bench(|| {
            let mut hasher = Xxh3::new();
            for chunk in data.chunks(BUF_SIZE) {
                hasher.update(black_box(chunk));
            }
            black_box(hasher.digest128());
        });
}

#[divan::bench(args = [MB, 16 * MB])]
fn blake3_chunked(bencher: divan::Bencher, size: usize) {
    let data = test_data(size);
    bencher
        .counter(divan::counter::BytesCount::new(size))
        .bench(|| {
            let mut hasher = Blake3Hasher::new();
            for chunk in data.chunks(BUF_SIZE) {
                hasher.update(black_box(chunk));
            }
            black_box(hasher.finalize());
        });
}

fn main() {
    divan::main();
}
