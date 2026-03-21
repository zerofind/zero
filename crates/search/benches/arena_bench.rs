use std::hint::black_box;

use search::arena::PathArena;

// -- fixtures -----------------------------------------------------------------

fn sample_paths(n: usize) -> Vec<String> {
    let dirs = ["src", "lib", "tests", "docs", "assets", "config", "build"];
    let exts = ["rs", "go", "py", "js", "json", "toml", "md"];
    (0..n)
        .map(|i| {
            let d1 = dirs[i % dirs.len()];
            let d2 = dirs[(i * 3) % dirs.len()];
            let ext = exts[i % exts.len()];
            format!("/Users/test/Documents/{d1}/{d2}/file_{i}.{ext}")
        })
        .collect()
}

// -- sequential push ----------------------------------------------------------

#[divan::bench(args = [10_000, 100_000])]
fn push_sequential(bencher: divan::Bencher, n: usize) {
    let paths = sample_paths(n);
    bencher.bench(|| {
        let mut arena = PathArena::with_capacity(n * 60);
        for p in &paths {
            black_box(arena.push(p).unwrap());
        }
        black_box(&arena);
    });
}

// -- push with free-list fragmentation ----------------------------------------

#[divan::bench(args = [10_000, 100_000])]
fn push_with_fragmentation(bencher: divan::Bencher, n: usize) {
    let paths = sample_paths(n);
    bencher.bench(|| {
        let mut arena = PathArena::with_capacity(n * 60);
        let mut refs: Vec<(u32, u16)> = Vec::with_capacity(n);

        // Fill
        for p in &paths {
            refs.push(arena.push(p).unwrap());
        }

        // Remove every other entry
        for i in (0..refs.len()).step_by(2) {
            arena.remove(refs[i].0, refs[i].1);
        }

        // Re-insert (exercises free-list best-fit)
        for i in (0..refs.len()).step_by(2) {
            black_box(arena.push(&paths[i]));
        }

        black_box(&arena);
    });
}

// -- random get ---------------------------------------------------------------

#[divan::bench(args = [10_000, 100_000])]
fn get_random(bencher: divan::Bencher, n: usize) {
    let paths = sample_paths(n);
    let mut arena = PathArena::with_capacity(n * 60);
    let refs: Vec<(u32, u16)> = paths.iter().map(|p| arena.push(p).unwrap()).collect();

    // Deterministic pseudo-random access order
    let access: Vec<usize> = (0..n).map(|i| (i * 7919) % n).collect();

    bencher.bench(|| {
        for &i in &access {
            let (off, len) = refs[i];
            black_box(arena.get(off, len));
        }
    });
}

fn main() {
    divan::main();
}
