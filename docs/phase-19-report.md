# Phase 19 Report

## Focus

This phase closed the three parity items that were explicitly held open:

1. `microcompact + context collapse archive`
2. provider-backed `session memory` constrained edit flow
3. `team memory` watcher/sync

## What changed

### Context pipeline

- Added `microcompact_session_messages(...)` in `crates/runtime/src/compact.rs`.
- `ContextOptimizer::prepare_session(...)` now runs:
  - tool-result budgeting
  - `snip`
  - `microcompact`
  - message collapse
  - full compaction
- Compaction now persists removed messages into `Session.context_collapse_archive`.
- Compaction also advances `context_collapse_commits` and `context_collapse_snapshot` so later turns and resumed sessions retain archived-state metadata.

### Session memory

- Provider-backed session-memory refresh no longer depends on a plain assistant-text fallback path as the primary mechanism.
- `refresh_current_session_memory_with_provider(...)` now launches an isolated nested runtime whose only writable tool is a constrained `edit_file` tool for the exact session-memory path.
- The update prompt and system prompt were aligned to the reference repos' `Edit-tool only, preserve template structure, stop after edits` model.
- Persisted session-memory state and wait-before-hydrate behavior remain in place and now work with the constrained updater flow.

### Team memory

- Added `crates/tools/src/team_memory_sync.rs`.
- Added config support for `teamMemorySync` in `crates/runtime/src/config.rs`.
- Added watcher/service lifecycle:
  - initial pull
  - delta push using per-entry content hashes
  - git remote repo detection
  - explicit write notifications
  - background polling/debounce loop
  - CLI status/pull/push commands
- Successful `write_file` and `edit_file` operations now notify the team-memory sync watcher when the target path is inside the team-memory root.

## Reference status

- `microcompact`: now aligned at the mechanism level the current reconstructed repos expose.
- `contextCollapse`: the reconstructed repos currently expose stubbed `contextCollapse` modules, so OpenCoWork's persisted archive/snapshot state is sufficient to cover the concrete behavior currently available there.
- `session memory`: now uses the reference-style constrained edit flow rather than a free-form provider summary path.
- `team memory`: now has the same core service shape as the reconstructed repos, though first-party OAuth specifics and some structured conflict/limit handling remain outside the current OpenCoWork implementation.

## Verification

- `cargo fmt`
- `cargo clippy --all-targets --all-features --target-dir d:\opencowork-target -- -D warnings`
- `cargo test --target-dir d:\opencowork-target`
- `cargo run --target-dir d:\opencowork-target -q -p opencowork-cli -- provider`
- `cargo run --target-dir d:\opencowork-target -q -p opencowork-cli -- tool-manifest`
- `cargo run --target-dir d:\opencowork-target -q -p opencowork-cli -- team-memory status`
