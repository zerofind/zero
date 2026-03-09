---
name: zero:changelog
description: Generate domain-focused changelog from staged git changes
user-invocable: true
allowed-tools: Read, Edit, Bash, Grep, Glob, Write
---

# Changelog Skill

Generate a changelog entry from staged changes for Zero. User-facing language, domain-grouped.

## Argument: $ARGUMENTS

- If provided (e.g. `0.6.2`), use `## v{version}` as the heading.
- If empty, read the version from `Cargo.toml` under `[workspace.package]` and use `## v{version}`.
- NEVER use `## [Unreleased]`.

## Steps

1. **Read context**:
   - `CHANGELOG.md` — previous entries for tone/style
   - `CLAUDE.md` Product Domains table — file-path-to-domain mapping
   - `FEATURES.md` section headings — canonical domain names

2. **Get staged changes** — run `git diff --cached --stat`, `git diff --cached --name-only`, `git diff --cached`. If nothing staged, stop.

3. **Read actual diffs** per domain. Understand what changed from the USER's perspective.

4. **Write entries** — one line per change:
   ```
   - {domain}: {what the user gets, in plain language}
   ```

5. **Reframe internals as user outcomes**:
   - BAD: `cache: file open frequency tracking via UsageStore in ControlDb`
   - GOOD: `search: frequently opened files rank higher in results`
   - BAD: `llm: thinking/reasoning mode with configurable budget`
   - GOOD: `ai: see the assistant's reasoning process in real-time`

6. **Group** into `New:`, `Fix:`, `Breaking:` sections (skip sections with no items; flat list if under 5 items total).

7. **Update CHANGELOG.md** — insert new `## v{version}` section at top, after `# Changelog`, before any existing `## v*`. If a section for the same version already exists, replace its contents. Never modify a different versioned section.

8. **Output** the changelog text directly.

## Rules

- Domain names come from FEATURES.md section headings, lowercased: `search:`, `ai:`, `app:`, `code:`, `cli:`.
- Use CLAUDE.md Product Domains table to map file paths to domains.
- One line per change. Plain text, no backticks or markdown.
- Internal module names (ControlDb, UsageStore, IndexManager, Arc, Mutex) never appear.
- Tests, lockfiles, and refactors are not changelog items.
- Frame everything as what the USER gets, not what the CODE does.
- Keep descriptions short — one clause, rarely two.
