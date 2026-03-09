---
name: zero:release
description: Bump version, changelog, features, clippy, fmt — stops before commit
user-invocable: true
allowed-tools: Read, Edit, Bash, Grep, Glob, Write, Skill
---

# Release Skill

Prepare a release for Zero. Does NOT commit — the user does that manually.

You are NOT done until you print the step 8 summary. If you haven't printed "Ready to commit", you haven't finished.

## Argument: $ARGUMENTS

- Empty → auto-determine bump (new features → minor, fixes only → patch)
- `patch` / `minor` / `major` → force that bump (major ONLY via explicit argument)
- A specific version like `0.7.0` → use exactly that

Major versions are never auto-inferred. Even if there are breaking changes, major bumps require explicit user intent.

## Steps

### 1. Stage and read version

- `git add .`
- Read `[workspace.package] version` from Cargo.toml → x.y.z
- `git tag -l "v{VERSION}"` — if tag missing, reuse version and skip bump; if tag exists, proceed with bump

### 2. Determine version and update Cargo.toml (only if bumping)

- If $ARGUMENTS is explicit (`patch`, `minor`, `major`, or a version string): use it.
- If $ARGUMENTS is empty, auto-determine from staged files:
  - `git diff --cached --diff-filter=A --name-only` — any added files in feature-relevant paths (src/, crates/*/src/services/, crates/*/src/views/, new CLI commands) → **minor**
  - Only modifications to existing files → **patch**
- Compute new version. Edit `[workspace.package] version` in Cargo.toml.
- `git add .` so sub-skills see the version change in staged diffs.

### 3. Generate changelog

Invoke `/zero:changelog {new_version}`.

After changelog completes, CONTINUE with step 4. You are still inside /zero:release. Remaining: features (4) → clippy (5) → fmt (6) → re-stage (7) → summary (8).

### 4. Update features

Invoke `/zero:features`.

After features completes, CONTINUE with step 5. You are still inside /zero:release. Remaining: clippy (5) → fmt (6) → re-stage (7) → summary (8).

### 5. Clippy

```
cargo clippy --fix --allow-dirty --allow-staged --workspace
cargo clippy --workspace
```
Fix remaining warnings manually if needed.

### 6. Format

```
cargo fmt --all
```

### 7. Re-stage

```
git add .
```

### 8. Summary

```
v{NEW_VERSION} ready ({BUMP_TYPE} — {reason})

{OLD} → {NEW}
changelog:  updated
features:   updated
clippy:     clean
fmt:        clean

Ready to commit:
  git commit -m "v{NEW_VERSION}"
  git tag v{NEW_VERSION}
```

Where reason is e.g. "auto: new features detected" or "explicit: user requested minor".

## Rules

- NEVER commit. NEVER push. NEVER create tags. The user does this manually.
- NEVER auto-infer major. Major bumps require explicit `major` argument.
- If clippy or fmt fails, fix the issues and re-run — don't skip.
- The version bump is ONLY in `[workspace.package] version` in root Cargo.toml.
- If there are no changes at all (nothing staged, nothing unstaged), say "nothing to release" and stop.
