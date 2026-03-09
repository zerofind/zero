---
name: zero:test
description: Write and audit tests for Zero modules
user-invocable: true
allowed-tools: Read, Glob, Grep, Bash, Edit, Write, Agent, Skill
---

# Test Writer

Analyze test gaps and write tests for Zero. Understands the `*_test.rs` co-located pattern, the module structure, and all existing conventions.

**First step, always:** Read `CLAUDE.md` at the project root for coding standards, architecture, and test structure rules. Those rules override anything here if they conflict.

## Argument: $ARGUMENTS

- **Empty** — analyze `git diff` changed files, identify missing tests, write them
- **`<module>`** (e.g. `hasher`, `index`, `cache`, `todo`, `transfer`) — audit and fill test gaps for that module tree
- **`<file_path>`** (e.g. `src/index/manager.rs`) — write tests for a specific file
- **`gaps`** — comprehensive gap report across all modules (report only, no writes)

## Test File Rules (Non-Negotiable)

### Placement

- Tests live in `*_test.rs` files co-located with source. **NEVER** inline `#[cfg(test)]` blocks.
- Test file for `src/foo/bar.rs` is `src/foo/bar_test.rs`.
- Test file for `src/foo/mod.rs` is `src/foo/mod_test.rs`.
- Test file for `crates/zero-ui/src/theme/mod.rs` is `crates/zero-ui/src/theme/mod_test.rs`.

### Module Declaration

After creating a test file, add it to the parent `mod.rs`:

```rust
#[cfg(test)]
mod bar_test;
```

If the module uses `#[path]` (needed for `mod.rs` and for test files whose name doesn't match the module — e.g. `rust.rs` testing via `rust_parser_test.rs`):

```rust
#[cfg(test)]
#[path = "mod_test.rs"]
mod mod_test;

// Or for parser-style submodule tests:
#[cfg(test)]
#[path = "rust_parser_test.rs"]
mod rust_parser_test;
```

### Imports

Always start with wildcard import from parent:

```rust
use super::*;
```

Then add what else you need:

```rust
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;
```

### Naming

- Functions: `test_<what>_<scenario>` or `<what>_<scenario>` (both are fine, be consistent within a file)
- Use section comments for grouping related tests:

```rust
// -- name matching -----------------------------------------------------------

#[test]
fn exact_match_highest_score() { ... }

#[test]
fn prefix_beats_substring() { ... }
```

### Structure: Arrange-Act-Assert

```rust
#[test]
fn chunked_copy_creates_parent_dirs() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "file.txt", b"data");
    let dest = dir.path().join("nested/deep/copy.txt");

    // Act
    let result = copy_chunked(&source, &dest, &Default::default());

    // Assert
    assert!(result.is_ok());
    assert!(dest.exists());
    assert_eq!(std::fs::read(&dest).unwrap(), b"data");
}
```

### Helpers

Define helpers locally in the test file. Return `TempDir` in tuples to keep it alive:

```rust
fn setup() -> (TempDir, CacheManager) {
    let dir = TempDir::new().unwrap();
    let mgr = CacheManager::open_at(dir.path()).unwrap();
    (dir, mgr)
}
```

### What to Test

For every function/method under test, cover:

1. **Happy path** — normal input, expected output
2. **Error path** — invalid input, missing files, corrupt data
3. **Edge cases** — empty input, zero-length, boundary values, Unicode
4. **Same-output differentiation** — when multiple inputs produce "correct" results, test that ranking/ordering/priority is right (like search scoring)

### What NOT to Do

- No async tests. All Zero tests are synchronous (even if the code uses tokio).
- No `#[ignore]` unless the test requires global state and can't run in parallel.
- No testing private functions directly. Test through the public API.
- No mock-heavy tests. Use real types, real tempfiles, real SQLite (in-memory).
- No tests that only assert `is_ok()` — also verify the returned value.
- No 50-line test setup. If setup grows, extract a local helper function.
- No hardcoded paths. Always use `TempDir`.

## Gap Categories

When analyzing, classify each gap:

1. **[MISSING-UNIT]** — Public function/method with no test at all
2. **[MISSING-ERROR]** — Happy path tested, error/edge cases not
3. **[WEAK-ASSERTION]** — Test exists but only checks `is_ok()` / `is_some()` without verifying the value
4. **[CRASH-VECTOR]** — Code path that can panic/crash: `unwrap()` on user input, unchecked index, division by zero
5. **[MISSING-MOD-DECL]** — Test file exists but not declared in `mod.rs` (won't compile)

## Execution Steps

### Step 1: Determine Scope

**No args (git diff mode):**
```bash
git diff --name-only HEAD
git diff --cached --name-only
```
Only consider `.rs` files. Exclude `_test.rs` files (those ARE tests). For each changed source file, check if a corresponding `_test.rs` exists and whether the changed functions have coverage.

**Module name:**
Glob `src/<module>/**/*.rs` and `crates/**/src/<module>/**/*.rs`. Identify all source files and their test counterparts.

**File path:**
Read the file, understand all public items, check the test file.

**gaps:**
Scan everything. Report only, don't write.

### Step 2: Inventory

For each source file in scope:

1. Read it. List all `pub fn`, `pub async fn`, `pub struct` with `impl` blocks, `pub enum` with methods.
2. Check if `<name>_test.rs` exists. If yes, read it and map which functions are covered.
3. Check `mod.rs` for `#[cfg(test)] mod <name>_test;` declaration.
4. Note any `unwrap()` calls on fallible operations (potential crash vectors).

### Step 3: Classify Gaps

For each uncovered item, assign a gap category. Prioritize:
- [CRASH-VECTOR] — fix these first, they're bugs
- [MISSING-UNIT] on public API — users hit these
- [MISSING-ERROR] — error paths are where bugs hide
- [WEAK-ASSERTION] — false confidence

### Step 4: Write Tests

For each gap:

1. Check if test file exists. If not, create it with the `use super::*;` header.
2. Add `#[cfg(test)] mod <name>_test;` to `mod.rs` if missing.
3. Write test functions following the conventions above.
4. Group with section comments.

**Test quality rules:**
- Test BEHAVIOR, not implementation. Assert what the user sees, not internal state.
- Error paths are more important than happy paths. A function that returns `Result` needs at least one `is_err()` test.
- For scoring/ranking/sorting: test relative ordering, not absolute values. Use `assert!(a > b)` not `assert_eq!(a, 1091)`.
- For filesystem operations: always use `TempDir`, never write to real paths.
- For SQLite: use `open_memory()` or `open_at(tempdir)`, never the real DB.
- Keep tests short. If a test is >30 lines, split it or extract helpers.

### Step 5: Verify

```bash
# Must compile
cargo check -p zero 2>&1 | head -30

# Tests must pass
cargo test <module>:: 2>&1

# No clippy warnings
cargo clippy -p zero --lib -- -D warnings 2>&1 | tail -10
```

If a crate test:
```bash
cargo test -p zero-ui 2>&1
cargo clippy -p zero-ui -- -D warnings 2>&1 | tail -10
```

### Step 6: Report

```
## Test Report: <scope>

### Written
- src/foo/bar_test.rs: 5 tests (3 new, 2 improved)
- src/foo/baz_test.rs: 3 tests (new file)

### Coverage Summary
| Module | Public Items | Tested | Error Paths | Status |
|--------|-------------|--------|-------------|--------|
| foo/bar | 8 | 7 | 4/5 | Good |
| foo/baz | 4 | 4 | 2/2 | Complete |

### Remaining Gaps
- [MISSING-ERROR] bar::validate() — no test for malformed input
- [CRASH-VECTOR] baz.rs:45 — unwrap() on user-supplied path
```

## Module Reference

### Well-Tested (use as patterns)
- `src/todo/` — comprehensive builder/CRUD/search coverage
- `src/transfer/` — filesystem + chunked copy + resume
- `src/hasher/` — algorithm identity + hash roundtrip
- `src/dedup/` — duplicate finding + streaming
- `src/disk/erase/` — wipe patterns + state machines
- `src/code/` — 73 tests across parsers (Rust, Go), formatting, element types, scanner, persistence, and CodeIndex integration

This list is approximate. Always verify by globbing for `*_test.rs` files and reading them — don't rely on this snapshot.

### Dev-Dependencies Available
```toml
[dev-dependencies]
tempfile = "3.13"
```
That's it. Tests use stdlib + `tempfile`. No test frameworks, no proptest, no mock libraries.

## Anti-Patterns

- **Testing formatting/display** instead of logic. If `Display` is the only thing tested, the actual computation is uncovered.
- **Asserting hardcoded scores** like `assert_eq!(score, 1091)`. Scores change. Assert relative ordering: `assert!(a > b)`.
- **Giant setup functions** that create 10 entities. Split into focused helpers.
- **Tests that mirror implementation** — if the test is just re-implementing the function and comparing, it catches nothing.
- **Testing internal state** — don't reach into private fields. Test through the public API.
- **Silent Result ignoring** — `let _ = might_fail();` in test code hides failures. Always unwrap or assert.
