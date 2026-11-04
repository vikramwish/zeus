# Project context and skills

`context.rs` is a read-only adapter over `Workspace::list` and `Workspace::read`.
It adds no filesystem resolver, execution engine, permission model, configuration,
or dependency. Only the declared private candidate is inspected.

## Public API

```rust
pub fn project_context(workspace: &Workspace) -> Result<String, String>;
pub fn list_skills(workspace: &Workspace) -> Result<serde_json::Value, String>;
pub fn read_skill(workspace: &Workspace, name: &str) -> Result<serde_json::Value, String>;
```

- `project_context`: finds exactly root `AGENTS.md` in the current workspace
  inventory and reads it through `Workspace`. Returns an empty string when no
  root file is inventoried; otherwise returns one serialized JSON object with
  `authority`, `path`, `content`, `sha256`, and `source_id`. No parent traversal,
  nested AGENTS discovery, alternative context filenames, or skill bodies are
  automatically loaded. A skipped root symlink/nonregular entry, unreadable,
  binary, or oversized root document is an error, not silently truncated text.
- `list_skills`: discovers exactly `skills/<name>/SKILL.md`, sorted by inventory
  path. Returns `{ "skills": [...], "skipped": [...], "truncated": false }`.
  Every skill entry has `name`, `path`, `bytes`, `sha256`, `source_id`, and
  `authority`; it has **no content field**. Discovery reuses `read_skill` to
  validate bytes and provenance, but does not put bodies into model context.
  Invalid names, binary/oversized/unreadable skill files and workspace-skipped
  entries below `skills` appear in sorted diagnostics. Nested category layouts
  are outside this small API, not recursively imported.
- `read_skill`: reads the exact file for a validated name, returning the same
  metadata plus the complete `content`. Names match
  `[A-Za-z0-9][A-Za-z0-9_-]{0,63}`. Names are never trimmed or normalized. Slashes,
  dots, drive prefixes, backslashes, Unicode aliases and traversal are rejected
  before lookup. Missing files and forbidden paths return errors.

Each source ID is `relative/path#<SHA-256 of the complete file bytes>`, matching
Zeus's existing evidence convention. Digests come from the current read, not
snapshot-time metadata. JSON serialization preserves the exact decoded UTF-8
content, including frontmatter; delimiters in the file cannot alter JSON fields.
A digest binds the observed bytes, not the truth or safety of the document.

## Limits and trust boundary

- Root guidance and individual skill bodies: **16 KiB UTF-8 bytes each**. A
  full `Workspace::read` is bounded by its existing 128 KiB limit before this
  tighter content limit is checked. Documents are rejected, never cropped.
- Catalog: **64 loadable skills** and **32 KiB serialized JSON**, including
  diagnostics. Either overflow fails the whole listing; there is no concealed
  pagination or partial-success catalog. Explicit named reads remain available.
- Workspace's existing inventory/read size, file-count, regular-file,
  symlink, sensitive-name and relative-path guards still apply. The adapter
  neither reads `Workspace::root` directly nor walks the host filesystem.
- `authority` is always `untrusted_repository_data`. Frontmatter names and
  `allowed-tools` declarations are inert text, not parsed configuration.
  Scripts, references, templates and links are not followed, installed, run,
  or fetched. Supporting files may be explicitly requested through the existing
  workspace read capability, with its normal boundary and independent digest.
- No home-directory memory, Hermes skills/config/secrets, plugins, environment
  credentials or implicit external roots are imported. There is no skill write,
  skill execution, automatic setup or permission elevation.

Runtime integration calls `project_context` before model work (after the baseline
no-op check), audits it and appends its labelled JSON to the task. `list_skills`
and `read_skill` are available to analysts and implementers; returned source IDs
are registered for findings. Runtime owns schemas, roles and context budgets. Keep the static
system rules in force and inject the returned JSON as explicitly untrusted
project guidance, never approval or a replacement system prompt. Register only
observed `source_id` values from project context and tool results; a later read
may legitimately have a different digest. These APIs are safe to expose as
read-only analyst capabilities; they do not grant write/check/MCP capabilities.
The existing workspace threat model still excludes hostile concurrent same-user
filesystem mutation; this module does not claim a new race-proof sandbox.

## Source inspection and design differences

Inspected the local Hermes checkout at
`bf0203a794a07aac3ff764972ab9087baa4e4f4e`:

- `agent/prompt_builder.py`, `_load_agents_md` and
  `build_context_files_prompt` (lines 1704–1778): provenance-labelled project
  sections, bounded merged context, several discovery sources and independent
  home identity loading. Zeus deliberately uses only root `AGENTS.md` and no
  identity/home import.
- `tools/skills_tool.py` (lines 1–5, 88–128): progressive list/view disclosure,
  profile-scoped roots, traversal validation and separate environment/setup
  behavior. Zeus borrows the list/read separation, not profile discovery or
  environment/setup behavior.
- Hermes `LICENSE`: MIT, copyright 2025 Nous Research. No upstream code was
  copied; this is a small independent Rust implementation using existing Zeus
  workspace primitives, not Hermes feature parity.

## Proof

`cargo test --offline --test project_context` exercises real candidate snapshots
and files: root-only injection, exact content/digests, current reads after edits,
original-source immutability, strict names, missing/binary/oversized documents,
UTF-8 byte boundaries, catalog limits including diagnostics, inert frontmatter
and script references, excluded directories and file/intermediate-directory
symlink escapes. `cargo clippy --offline --test project_context -- -D warnings`
checks this public-crate integration target. These are deterministic local
filesystem proofs, not evidence of model adherence to malicious instructions.
