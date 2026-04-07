# Phase 15 Report

## Scope

This phase continued direct parity work against:

1. `claude-code-best/claude-code`
2. `ChinaSiro/claude-code-sourcemap`

The focus stayed on the three-layer memory stack and its compaction behavior.

## Delivered

- Added provider-backed relevant-memory selection on top of the existing bounded memory scan.
  - When a provider is available, OpenCoWork now asks the model to choose relevant memory files from a manifest instead of relying only on heuristic scoring.
  - The heuristic path remains as fallback when provider-side selection is unavailable or fails.
- Added a reference-style session-memory threshold state machine.
  - Initialization threshold: `10_000`
  - Minimum growth between updates: `5_000`
  - Tool-call threshold: `3`
  - Stale extraction timeout: `60s`
- Added team-memory entrypoint loading through `memory/team/MEMORY.md`.
- Added dedicated session-memory compaction.
  - OpenCoWork now prefers current session memory during compaction instead of always falling back to transcript-only summary compaction.
  - The kept tail now follows reference-style minimums:
    - preserve at least `10_000` estimated tokens
    - preserve at least `5` text-bearing messages
    - stop expansion at `40_000` estimated tokens
  - Tool-use / tool-result pairs are kept together when choosing the preserved tail.
  - Oversized session-memory sections are truncated before insertion into compact summary context.
- Added regression coverage for:
  - provider-backed relevant-memory selection
  - session-memory-first compaction
  - tool-pair preservation during session-memory compaction
  - oversized session-memory truncation

## Files

- [compact.rs](d:\新建程序项目\opencowork\crates\runtime\src\compact.rs)
- [memory.rs](d:\新建程序项目\opencowork\crates\runtime\src\memory.rs)
- [session.rs](d:\新建程序项目\opencowork\crates\runtime\src\session.rs)
- [session_memory.rs](d:\新建程序项目\opencowork\crates\runtime\src\session_memory.rs)
- [lib.rs](d:\新建程序项目\opencowork\crates\app\src\lib.rs)
- [main.rs](d:\新建程序项目\opencowork\crates\opencowork-cli\src\main.rs)

## Validation

- `cargo fmt`
- `cargo test --target-dir "$env:TEMP\opencowork-target"`
- `cargo clippy --all-targets --all-features --target-dir "$env:TEMP\opencowork-target" -- -D warnings`

## Remaining gaps

- Session memory is still refreshed synchronously at turn end. The reference repositories use a forked/background extractor path.
- Provider-backed relevant-memory recall now exists, but it still uses the current provider path, not a dedicated side-query model route.
- Team memory sync, watcher behavior, secret-guard checks, and server reconciliation are still missing.
- Auto-memory extraction and nightly distillation into `MEMORY.md` and topic files are still missing.
