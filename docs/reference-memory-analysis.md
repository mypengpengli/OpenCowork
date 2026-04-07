# Reference Memory Analysis

## Canonical references

Memory behavior should continue to be checked in this order:

1. `claude-code-best/claude-code`
2. `ChinaSiro/claude-code-sourcemap`

The local `.tmp/claw-code-main` Rust rewrite remains useful for architecture ideas, but it is not the authority for Claude Code memory behavior.

## What the reference repos actually do

The reconstructed Claude Code repositories split memory into three layers.

### 1. Instruction memory and nested memory

Primary files:

- `src/utils/claudemd.ts`
- `src/utils/attachments.ts`

Behavior:

- Load managed, user, project, and local instruction memory.
- Load `.claude/CLAUDE.md` and `.claude/rules/**/*.md`.
- Load nested memory files for directories reached by touched files.
- Keep this layer separate from query-time relevant-memory recall.

### 2. Current session memory

Primary files:

- `src/services/SessionMemory/sessionMemory.ts`
- `src/services/SessionMemory/sessionMemoryUtils.ts`
- `src/services/compact/sessionMemoryCompact.ts`

Behavior:

- Maintain a markdown summary file for the active conversation.
- Update that file asynchronously in the background.
- Compact session memory with its own thresholds instead of treating it as part of normal transcript compaction.

### 3. Relevant memory recall

Primary files:

- `src/memdir/findRelevantMemories.ts`
- `src/utils/attachments.ts`
- `src/memdir/paths.ts`

Behavior:

- Scan the project's persistent memory directory.
- Exclude `MEMORY.md`, because it is already loaded separately.
- Use a model-side selector to choose up to five relevant memory files.
- Surface the recalled memories as their own attachment type with size caps and session-level throttling.

## Current OpenCoWork mapping

OpenCoWork now maps those layers like this:

- Instruction memory and nested memory:
  - [prompt.rs](d:\新建程序项目\opencowork\crates\runtime\src\prompt.rs)
- Current session memory:
  - [memory.rs](d:\新建程序项目\opencowork\crates\runtime\src\memory.rs)
  - [session.rs](d:\新建程序项目\opencowork\crates\runtime\src\session.rs)
- Relevant memory recall:
  - [memory.rs](d:\新建程序项目\opencowork\crates\runtime\src\memory.rs)

## What is aligned now

- `.claude/CLAUDE.md` and `.claude/rules/**/*.md` now load into the instruction surface.
- Touched-path-driven nested instruction discovery now exists.
- A durable project-memory entrypoint now exists via `MEMORY.md`.
- A team-memory entrypoint now exists via `memory/team/MEMORY.md`.
- Current-session memory is now persisted and re-injected, using the same section template shape as the reference session-memory notes.
- Relevant memory recall now exists at request time.
- Provider-backed relevant-memory selection now exists when a provider is available, with heuristic recall kept only as fallback.
- `MEMORY.md` entrypoint truncation now follows the reference-style 200-line / 25 KB bound.
- Relevant-memory scanning now follows the reference-style header-first approach with a 30-line header scan and bounded 200-line / 4 KB surfacing.
- Session-memory threshold state now follows the reference defaults of `10k / 5k / 3`.
- Session-memory compaction now exists as its own path, with reference-style preserved-tail thresholds and tool-pair preservation.
- Team-memory write protection now exists in the tool layer, including path scoping plus secret-content rejection.
- Session memory is now background-scheduled and rehydrated from disk before the next turn's context preparation.
- Session memory now uses a provider-backed background updater by default, with deterministic refresh kept only as fallback.
- Session-memory state now tracks trigger cadence separately from the compaction boundary, matching the reference repos' split between "last trigger" and "last summarized" behavior.
- Session-memory completion state is now persisted beside the summary file, then merged back into the next turn before prompt assembly.
- Prompt preparation now waits briefly for an in-flight session-memory refresh, mirroring the reference repos' "wait before consuming compact/session memory" behavior.
- Provider-backed session-memory refresh now runs through a constrained `edit_file`-only nested runtime, which mirrors the reference repos' "forked agent with a single allowed edit target" behavior.
- Team-memory sync now has a watcher/service layer with initial pull, delta push, repo-scoped sync state, and explicit write notifications from file tools.
- Team-memory writes now feed the watcher immediately after successful `write_file` and `edit_file` calls, mirroring the reference repos' PostToolUse notify path.

## What is still behind the reference repos

- Relevant-memory recall does not yet use a dedicated side-query model route.
- Auto-memory log extraction and distillation are still absent.
- Team-memory sync is now present, but it still lacks the reference repos' first-party OAuth-specific auth stack and some structured conflict/413 handling.
