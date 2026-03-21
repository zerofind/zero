---
name: zero:web-changelog
description: Draft a landing page changelog post for a version
user-invocable: true
allowed-tools: Read, Edit, Bash, Grep, Glob, Write, Agent
---

# Web Changelog Skill

Draft a single landing page changelog post for a Zero release. One post per version. The hero feature gets the title, supporting features get sections, minor items get a list at the end.

## Argument: $ARGUMENTS

**Required: a version or version range** from CHANGELOG.md.

Accepted formats:
- Single version: `v0.7.0` or `0.7.0`
- Range: `v0.4.5-v0.4.9` — reads all versions from v0.4.5 through v0.4.9 and combines them into one post. Use this for rapid small releases that belong together.

The version is the source of truth — it defines exactly which changes are new.

If no argument provided, read `CHANGELOG.md` and cross-reference with existing posts in `../zero-landingpage/src/content/changelog/` to list versions that still need a web post.

## Steps

### 1. Read the version(s)

- **Single version**: Read the `## v{version}` section from `CHANGELOG.md`.
- **Range**: Read ALL `## v{x.y.z}` sections that fall within the range. Combine all entries into one pool — the post covers everything across those versions.
- **Get the date from git**, not from guessing. Run `git log --format="%ai" --grep="^v{version}$" -1` to get the commit date for the version tag. For ranges, use the latest version's date. Never hardcode or guess dates.
- Read ALL existing web posts in `../zero-landingpage/src/content/changelog/*.mdx` — frontmatter AND body — to understand what's already published. Don't repeat what a previous post already said.

### 2. Rank the changes

Classify every changelog entry into one of three tiers:

- **Hero** (1 only) — the most impactful or exciting change. This gets the post title, the summary hook, and the opening paragraph. Pick the one a user would tell a friend about.
- **Supporting** (2–4) — notable features or improvements that deserve their own ### section with 2–4 bullets.
- **Minor** — small improvements and fixes. These go in a flat bullet list under "### Also in this update" at the end. Skip pure internal changes (refactors, dependency bumps, test-only changes).
- **Skip entirely** — changes that don't belong on a landing page: cross-platform compilation fixes, telemetry internals, CLI-only flags, dependency bumps. These are real work but not what users come to a changelog page to read.

### 3. Research

- Read relevant source code for the hero and supporting features. Understand what the user actually sees and does — not just what the code does.
- Look at UI code (views, components) to understand the experience.
- Check for keyboard shortcuts, CLI commands, or workflows.
- The goal: be able to describe what it FEELS like to use each feature, not just what it IS.

### 4. Propose

Before writing any file, output a proposal:

```
Hero:       {the feature that gets the title}
Title:      {proposed title — 1-5 words}
Date:       {YYYY-MM-DD}
Slug:       {url-slug}
Summary:    {one-sentence hook}

Supporting sections:
- {heading} — covers: {changelog entries}
- {heading} — covers: {changelog entries}
- ...

Minor (bulleted at end):
- {entry}
- ...

Skipping (already covered / internal):
- {entry} — reason
```

**Stop and ask the user** to confirm or adjust before writing.

### 5. Write

Write the MDX file to `../zero-landingpage/src/content/changelog/{date}-{slug}.mdx`.

Structure:

```mdx
---
title: {title}
date: "{YYYY-MM-DD}"
slug: {slug}
summary: {summary}
---

{opening — 1-2 sentences that make you want the hero feature. Paint a scenario.}

### {Feature Name} — {benefit}

- **{outcome}** — {how, briefly}
- ...

### {Feature Name} — {benefit}

{1-2 sentences setting up why this matters}

- **{outcome}** — {how}
- ...

### {Short benefit heading}

...

### Also in this update

- {minor item as one-line benefit}
- ...
```

### 6. Output

Show the full post content inline so the user can review before committing.

---

## Voice & Tone

### The one rule

Every sentence answers **"why should I care?"** — never "what did we build?"

You are talking TO the user ABOUT their experience. Not announcing what the engineering team shipped.

### Title

Named after the hero feature. 1–5 words. A concept or benefit, not a feature spec.

No implementation words. Ask: would someone who doesn't write code understand this title? If not, rewrite it.

| Don't write | Write instead |
|---|---|
| AI agent with MCP server | Ask AI about your files |
| Resumable sync engine | Sync that never loses progress |
| Cloud storage backends | Your files on S3, Dropbox, and more |
| Code intelligence for Rust & Go | Find any function by name |
| Multiple workspaces with switching | Spaces for every side of your work |

### Summary (the hook)

One sentence shown on the changelog index page. Its only job is to make someone click "Read more."

- Lead with the hero, hint at the rest: "Organize your projects into separate spaces — plus smarter search, AI reasoning, and more."
- Speak to the user's situation, not the feature's architecture.
- Use "you/your" — the reader is the subject, not Zero.

| Don't write | Write instead |
|---|---|
| New workspaces feature with independent state and keyboard switching | Organize your projects into separate spaces. Each one remembers where you left off. |
| Our built-in AI now supports MCP tool calling | Ask a question about your files. The AI searches, reads, and answers — all without leaving Zero. |
| 36 cleanup categories across 8 groups | Reclaim gigabytes from build artifacts, caches, and dev garbage you forgot about. |

### Body copy

- **Open with a real, specific scenario.** Not "You have a photography project and a codebase." Use concrete examples from the user's world: "You work on Zero — the core repo, the landing page, the design files. You work on Tell — six SDKs, a cloud platform, documentation." Specificity makes it tangible.
- **Bullets start with what the user sees or does.** Not "MCP server exposes search" but "Ask the AI to find files — it searches your index directly."
- **Technical precision in user language.** "Every file verified with a cryptographic checksum" not "BLAKE3 hash verification pipeline." Accuracy matters — jargon doesn't.
- **Short paragraphs.** One idea each. If a paragraph has "and" connecting two ideas, split it.
- **Avoid "we."** Use "Zero" (sparingly) or address the user directly with "you."
- **Show, don't list.** Describe a scenario or give an example. "Type `AuthProvider` and find it across every Rust project on your machine" beats "Cross-project structural symbol search."

### Section headings

Use the pattern: **`Feature Name — benefit`**. The feature name anchors what it is, the subtitle says why you care.

For major sections (hero + supporting):
- `Workspaces — a space per project`
- `Code Intelligence — a blueprint of your codebase`
- `Smarter search`
- `Watch the AI think`

Short benefit-only headings are fine for smaller sections where the feature name IS the benefit (e.g. "Smarter search").

| Don't write | Write instead |
|---|---|
| What's new | (never — the whole post is what's new) |
| A workspace for everything you do | Workspaces — a space per project |
| Search your code the way you think about it | Code Intelligence — a blueprint of your codebase |
| Implementation | (never) |
| Technical details | How it works (only if explaining internals briefly) |
| Also in this update | Also in this update (this one is fine — it's the catch-all) |

### Banned words

Never use in titles, summaries, or body copy:

`engine`, `backend`, `frontend`, `protocol`, `daemon`, `parser`, `module`, `crate`, `bitmap`, `index` (as a noun), `pipeline`, `architecture`, `implementation`, `refactor`, `schema`, `migration`, `runtime`, `abstraction`, `API` (unless user-facing), `provider` (say the service name)

### Transformation examples

| Changelog line | Web copy |
|---|---|
| `search: frequently opened files rank higher in results` | Files you open often float to the top. Zero learns what matters to you. |
| `app: git status badges on sidebar bookmarks` | See which folders have uncommitted changes — right in the sidebar, without opening a terminal. |
| `search: filter by size range with --min-size and --max-size` | Looking for the large files eating your disk? Filter by size to find them instantly. |
| `app: multiple workspaces with Cmd+1-9 switching` | Keep work, personal, and side projects in separate spaces. Switch with Cmd+1-9. |
| `code: structural code search — find functions, types, traits by name` | Search your code the way you think about it — by function name, type, or trait. Not grep. |
| `ai: see the assistant's reasoning process in real-time` | Watch the AI think. See its reasoning unfold as it works through your question. |
| `app: search results show a Location column` | Search results now show you where each file lives — no more guessing which `config.yaml` is which. |

---

## File conventions

- **Filename**: `{YYYY-MM-DD}-{slug}.mdx` — date is the publication date, not the release date
- **Slug**: lowercase, hyphenated, 2-4 words, no version numbers
- **Path**: `../zero-landingpage/src/content/changelog/`
- **One post per version** (or per version range for rapid small releases). Hero feature titles the post. Everything else fits inside it.
- **The index.ts auto-discovers** all `.mdx` files via `import.meta.glob` — no manual registration needed
