# Phase 14 Report

## Scope

This phase continued parity work against:

1. `claude-code-best/claude-code`
2. `ChinaSiro/claude-code-sourcemap`

The focus was the reference memory stack, especially the three-layer structure that the reconstructed repositories actually use:

- instruction memory and nested memory loading
- current session memory
- relevant memory recall

## Reference findings

The reference repositories do not implement "memory" as one flat subsystem.

- `src/utils/claudemd.ts` handles managed, user, project, and local instruction memory, plus nested memory loading from touched paths and `.claude/rules`.
- `src/services/SessionMemory/*` maintains a current-session markdown memory file.
- `src/memdir/findRelevantMemories.ts` and `src/utils/attachments.ts` perform query-time recall of relevant persistent memories and inject them separately from the main instruction files.

That means the effective runtime memory model is three parallel layers:

1. durable instruction memory
2. current-session working memory
3. query-time relevant memory recall

## Delivered

- Added a dedicated runtime memory module in [memory.rs](d:\新建程序项目\opencowork\crates\runtime\src\memory.rs).
- Added project-scoped memory roots that mirror the reference idea of a config-home `projects/<project>/memory` layout.
- Added persistent project memory entrypoint loading from `MEMORY.md`.
- Added current-session memory persistence and prompt injection.
  - Session memory is now refreshed after live turns.
  - Session files now carry an optional session id and current-session memory payload.
  - Saved sessions now persist their id into the session JSON instead of losing it on disk.
  - The stored notes now use the same section template shape as the reference `SessionMemory/prompts.ts`.
- Added query-time relevant memory recall.
  - The runtime now scans project memory markdown files below the memory root.
  - It now follows the reference scanning shape more closely: frontmatter/header scan first, newest-first candidate cap, then bounded content surfacing.
  - It injects the top relevant memories into prompt assembly with explicit labels.
- Added nested instruction-memory discovery for touched files.
  - Nested `CLAUDE.md`, `.claude/CLAUDE.md`, and `.claude/rules/**/*.md` files now load when touched files descend into those directories.
- Extended the base instruction discovery path so `.claude/CLAUDE.md` and `.claude/rules/**/*.md` are part of the main instruction-memory surface as well.
- Removed duplicated touched-path extraction logic from the CLI/app layers and pushed that behavior into the runtime memory module.
- Replaced custom memory constants with reference-style bounds where the behavior was clear:
  - `MEMORY.md` entrypoint truncation at 200 lines / 25 KB
  - relevant-memory header scan at 30 lines
  - relevant-memory surfacing at 200 lines / 4 KB
  - session-memory template budget aligned to the reference 12K-token class structure

## Validation

- `cargo fmt`
- `cargo test --target-dir "$env:TEMP\opencowork-target"`
- `cargo clippy --all-targets --all-features --target-dir "$env:TEMP\opencowork-target" -- -D warnings`

## Remaining gaps

- Relevant-memory selection is still heuristic. The reference repositories use a side-query selector to choose memory files more accurately.
- Session memory is now persisted and injected, but it is not yet maintained by a background extractor sub-agent like the reference implementation.
- Team memory and auto-memory log distillation are still missing.
- The current session-memory storage path is project-scoped and session-aware when an id exists, but it is still simpler than the reference repo's full per-session directory model.
