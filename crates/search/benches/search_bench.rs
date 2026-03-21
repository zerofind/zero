use std::hint::black_box;

use search::node::{FileNode, NodeType};
use search::scoring::{self, NodeContext};
use search::type_index::FileTypeCategory;
use search::{SearchIndex, SearchQuery};

// -- fixtures -----------------------------------------------------------------

const EXTENSIONS: &[&str] = &[
    "rs", "go", "py", "js", "ts", "json", "toml", "yaml", "md", "txt", "jpg", "png", "mp4", "pdf",
    "zip", "csv", "html", "css", "sh", "sql",
];

const DIRS: &[&str] = &[
    "src",
    "lib",
    "tests",
    "docs",
    "assets",
    "config",
    "scripts",
    "data",
    "node_modules",
    "target",
    ".cache",
    "vendor",
    "build",
    "out",
    "tmp",
];

fn synthetic_nodes(n: usize) -> Vec<FileNode> {
    let mut nodes = Vec::with_capacity(n);
    for i in 0..n {
        let ext = EXTENSIONS[i % EXTENSIONS.len()];
        let dir1 = DIRS[i % DIRS.len()];
        let dir2 = DIRS[(i * 7) % DIRS.len()];
        let depth = (i % 5) + 2;
        let path = if depth <= 2 {
            format!("/Users/test/Documents/{dir1}/file_{i}.{ext}")
        } else if depth <= 3 {
            format!("/Users/test/Documents/{dir1}/{dir2}/file_{i}.{ext}")
        } else {
            format!("/Users/test/Documents/{dir1}/{dir2}/deep/nested/file_{i}.{ext}")
        };
        let size = ((i * 1337) % 10_000_000) as u64;
        let mtime = 1_700_000_000u64.wrapping_add((i * 3600) as u64);
        nodes.push(FileNode::new(path, NodeType::File, size, mtime));
    }
    nodes
}

fn build_index(n: usize) -> SearchIndex {
    let nodes = synthetic_nodes(n);
    let mut index = SearchIndex::with_capacity(n);
    for node in nodes {
        index.insert(node);
    }
    index.finalize();
    index
}

// -- index build + finalize ---------------------------------------------------

#[divan::bench(args = [10_000, 100_000, 500_000])]
fn index_build(bencher: divan::Bencher, n: usize) {
    let nodes = synthetic_nodes(n);
    bencher.bench(|| {
        let mut index = SearchIndex::with_capacity(n);
        for node in nodes.clone() {
            index.insert(node);
        }
        index.finalize();
        black_box(&index);
    });
}

// -- persistence round-trip ---------------------------------------------------

#[divan::bench(args = [10_000, 100_000, 500_000])]
fn index_roundtrip(bencher: divan::Bencher, n: usize) {
    let index = build_index(n);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bench.zidx");

    bencher.bench(|| {
        search::persistence::save_index(&index, &path).unwrap();
        let loaded = search::persistence::load_index(&path).unwrap();
        black_box(&loaded);
    });
}

// -- text query ---------------------------------------------------------------

#[divan::bench(args = [10_000, 100_000, 500_000])]
fn query_prefix(bencher: divan::Bencher, n: usize) {
    let index = build_index(n);
    bencher.bench(|| {
        let results = index.search("file_1", 100);
        black_box(&results);
    });
}

#[divan::bench(args = [100_000, 500_000])]
fn query_substring(bencher: divan::Bencher, n: usize) {
    let index = build_index(n);
    bencher.bench(|| {
        let results = index.search("_42", 100);
        black_box(&results);
    });
}

// -- type-filtered query (roaring bitmap) -------------------------------------

#[divan::bench(args = [100_000, 500_000])]
fn query_type_filter(bencher: divan::Bencher, n: usize) {
    let index = build_index(n);
    bencher.bench(|| {
        let q = SearchQuery::by_type(FileTypeCategory::Images, 100);
        let results = index.query(q);
        black_box(&results);
    });
}

#[divan::bench(args = [100_000, 500_000])]
fn query_text_with_type(bencher: divan::Bencher, n: usize) {
    let index = build_index(n);
    bencher.bench(|| {
        let q = SearchQuery::text("file", 100).with_type(FileTypeCategory::Code);
        let results = index.query(q);
        black_box(&results);
    });
}

// -- recent files query -------------------------------------------------------

#[divan::bench(args = [100_000, 500_000])]
fn query_recent(bencher: divan::Bencher, n: usize) {
    let index = build_index(n);
    bencher.bench(|| {
        let q = SearchQuery::recent(100);
        let results = index.query(q);
        black_box(&results);
    });
}

// -- scoring ------------------------------------------------------------------

#[divan::bench]
fn scoring_shallow_path(bencher: divan::Bencher) {
    let path = "/Users/test/Documents/report.pdf";
    let now = 1_700_100_000u64;
    bencher.bench(|| {
        let ctx = NodeContext::new(black_box(path), 1_700_000_000);
        black_box(scoring::score_result(&ctx, "report", now));
    });
}

#[divan::bench]
fn scoring_deep_noisy_path(bencher: divan::Bencher) {
    let path = "/Users/test/Documents/project/node_modules/.cache/babel/transform/file.js";
    let now = 1_700_100_000u64;
    bencher.bench(|| {
        let ctx = NodeContext::new(black_box(path), 1_700_000_000);
        black_box(scoring::score_result(&ctx, "file", now));
    });
}

fn main() {
    divan::main();
}
