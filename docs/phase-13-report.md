# Phase 13 Report

## Scope

This phase continued parity work against the two primary references:

1. `claude-code-best/claude-code`
2. `ChinaSiro/claude-code-sourcemap`

The focus was the pre-request context pipeline and deferred-tool behavior, not terminal-only UI work.

## Delivered

- Added reference-style context-window override handling in the runtime model-context helpers.
  - `CLAUDE_CODE_AUTO_COMPACT_WINDOW` now caps the working context window.
  - `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` now lowers the autocompact threshold when requested.
- Added a `snip`-style history pre-pass in the context optimizer.
  - The optimizer now runs: tool-result budgeting -> snip -> message collapse -> full compaction.
  - This lets oversized old prefixes fall out before summary compaction is necessary.
- Added reference-style `ToolSearch` mode handling.
  - `ENABLE_TOOL_SEARCH` now supports `tst`, `tst-auto`, and `standard` semantics through:
    - unset / `true` / `auto:0` -> defer builtin specialized tools
    - `auto` / `auto:N` -> inline deferred tools only when under the 10% threshold
    - `false` / `auto:100` -> inline deferred tools directly
- Added manifest diagnostics for deferred-tool sizing and threshold decisions.
- Kept the previously added skill listing and persisted tool-result preview work, so the context and tool surfaces now move closer together toward the reference behavior.

## Validation

- `cargo fmt`
- `cargo test --target-dir "$env:TEMP\opencowork-target"`
- `cargo clippy --all-targets --all-features --target-dir "$env:TEMP\opencowork-target" -- -D warnings`
- `cargo run --target-dir "$env:TEMP\opencowork-target" -q -p opencowork-cli -- provider`
- `cargo run --target-dir "$env:TEMP\opencowork-target" -q -p opencowork-cli -- tool-manifest`
- `ENABLE_TOOL_SEARCH=auto` and `ENABLE_TOOL_SEARCH=false` were also checked against `tool-manifest` to confirm mode switching.

## Remaining gaps

- `snip` is present, but it is still simpler than the source-mapped implementation. It does not yet emit or replay boundary metadata.
- `contextCollapse` is still not reference-grade. OpenCoWork still lacks a persisted collapse store and projected archived view.
- `ToolSearch` mode logic is aligned, but deferred MCP deltas and post-compaction discovered-tool reconstruction are still missing.
- The next large step should be the UI shell, but only after the remaining context-pipeline and deferred-tool parity work is stable enough to avoid rework in the web/EXE layer.
