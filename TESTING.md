# Testing

all tests → `cargo test`
specific module → `cargo test hasher::`
specific crate → `cargo test -p zero-watcher`
don't stop on failure → `cargo test --no-fail-fast`
lint → `cargo clippy --workspace --all-targets`
format → `cargo fmt`
benchmark → `./scripts/benchmark.sh` (needs release build)
benchmark existing folder → `./scripts/benchmark.sh /path/to/folder`
benchmark N runs → `./scripts/benchmark.sh -r 5 ~/Documents`
