---
name: zero:features
description: Update FEATURES.md to reflect current project state
user-invocable: true
allowed-tools: Read, Edit, Bash, Grep, Glob, Write
---

# Features Skill

Update FEATURES.md to reflect current state of Zero. User-facing language, domain-grouped, exhaustive.

## Purpose

FEATURES.md is a precise catalog of capabilities from the user's perspective. It serves both users (what can Zero do?) and developers (feature inventory for planning). It is NOT marketing copy (that's WHY-ZERO.md).

## Steps

1. **Read context**:
   - `FEATURES.md` — current state
   - `CHANGELOG.md` — recent changes to cross-reference
   - `CLAUDE.md` Product Domains table — file-path-to-domain mapping
   - `internal/WHY-ZERO.md` if it exists — tone reference

2. **Scan for changes** — check staged diffs (`git diff --cached --name-only`) and recent commits (`git log --oneline -20`). Read diffs for changed domains.

3. **For each FEATURES.md section**, verify:
   - All shipped features listed?
   - Status markers accurate? (unmarked = shipped, `(partial)`, `(planned)`)
   - Descriptions user-facing, no internal jargon?

4. **Add new sections** only when a genuinely new capability area was added that doesn't fit existing sections. Section names must match CLAUDE.md Product Domains table.

5. **Update with minimal edits.** Don't rewrite unchanged sections.

## Format

```markdown
## Section Name

- Feature name — concise description of what the user gets.
- Partial feature — description. (partial)
- Planned feature — description. (planned)
```

## Rules

- Every bullet is a user-visible capability, not an implementation detail.
- BAD: `UsageStore with BTreeMap in ControlDb collection 5`
- GOOD: `Frequency tracking — files opened more often rank higher in future searches`
- Em-dash (—) separates feature name from description.
- Status markers at END of line: `(partial)`, `(planned)`, or nothing.
- Measurable details are good: "1.7M files in 83ms", "36 categories", "28 languages".
- No internal module names, crate names, or struct names.
- One bullet per capability. No nested sub-bullets.
- Section order follows user importance: Search and AI before Todo.
- After updating, verify every item in CHANGELOG.md's latest version has a corresponding FEATURES.md bullet.
