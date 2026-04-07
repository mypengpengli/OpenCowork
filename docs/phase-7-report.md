# Phase 7 Report

## Scope

This phase focused on copying the reference project's stronger context and tool orchestration ideas into OpenCoWork without dragging in terminal-only UI work that does not matter for the future web or EXE shell.

## Implemented

- Added deferred builtin tool exposure in [crates/tools/src/lib.rs](d:\新建程序项目\opencowork\crates\tools\src\lib.rs), keeping `Skill` out of the default manifest until `ToolSearch` activates it.
- Wired `SkillCatalog` into the global registry from the CLI composition layer in [crates/opencowork-cli/src/main.rs](d:\新建程序项目\opencowork\crates\opencowork-cli\src\main.rs).
- Added ancestor-chain instruction discovery, content dedupe, and per-file truncation in [crates/runtime/src/prompt.rs](d:\新建程序项目\opencowork\crates\runtime\src\prompt.rs).
- Added prompt-side reinjection of active skill instructions based on prior `Skill` tool results in [crates/runtime/src/prompt.rs](d:\新建程序项目\opencowork\crates\runtime\src\prompt.rs).
- Upgraded compaction to preserve and merge previously compacted summaries in [crates/runtime/src/compact.rs](d:\新建程序项目\opencowork\crates\runtime\src\compact.rs).
- Updated the context optimizer to use formatted compact summaries in [crates/runtime/src/context.rs](d:\新建程序项目\opencowork\crates\runtime\src\context.rs).

## Behavior changes

- Default tool manifests are smaller. `tool-manifest` now shows core tools plus `ToolSearch`; deferred tools are activated on demand.
- Prompt planning now inherits root and nested instruction files more reliably when work is started from a subdirectory.
- A skill loaded during a session can influence later turns without requiring the model to reload it every time, as long as the relevant tool result is still present in session history.
- Re-compacting a long session no longer erases earlier compacted context.

## Validation

- `cargo fmt`
- `cargo test --target-dir "$env:TEMP\opencowork-target"`
- `cargo run --target-dir "$env:TEMP\opencowork-target" -q -p opencowork-cli -- tool-manifest`
- `cargo run --target-dir "$env:TEMP\opencowork-target" -q -p opencowork-cli -- prompt-plan`

## Remaining gap

- Deferred activation is in place for `Skill`, but heavier subsystems such as LSP-style semantic tools are not yet wired into the same lazy path.
- Skill reinjection currently relies on prior tool-result payloads, not a dedicated typed memory store.
- Context selection still uses heuristic token estimation instead of provider-native counts.
