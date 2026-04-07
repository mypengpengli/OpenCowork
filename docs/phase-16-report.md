# Phase 16 Report

## Scope

This phase continued direct parity work against:

1. `claude-code-best/claude-code`
2. `ChinaSiro/claude-code-sourcemap`

The focus was the team-memory boundary inside the tool layer.

## Delivered

- Added a dedicated team-memory guard module in [team_memory.rs](d:\新建程序项目\opencowork\crates\tools\src\team_memory.rs).
- Added reference-style team-memory path detection against the project-scoped `memory/team` root.
- Added symlink-aware containment checks for team-memory write targets.
- Added a curated high-confidence secret scanner for team-memory writes, aligned with the reference repositories' `teamMemSecretGuard` / `secretScanner` behavior.
- Wired the guard into builtin file tools:
  - [lib.rs](d:\新建程序项目\opencowork\crates\tools\src\lib.rs)
  - `write_file`
  - `edit_file`
- Added regression tests for:
  - safe non-secret team-memory writes
  - rejecting GitHub PAT-like content in team memory
  - ignoring non-team-memory file paths

## Validation

- `cargo fmt`
- `cargo test --target-dir "$env:TEMP\opencowork-target"`
- `cargo clippy --all-targets --all-features --target-dir "$env:TEMP\opencowork-target" -- -D warnings`

## Remaining gaps

- Team-memory sync/watcher behavior is still missing.
- Team-memory read/search activity is not yet surfaced as message-level counters.
- Team-memory write protection now exists, but the broader sync protocol and remote conflict handling from the reference repositories are still absent.
