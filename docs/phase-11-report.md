# Phase 11 Report

This phase focused on the small context and tool-handling mechanisms that matter more than surface features when the runtime is under real pressure.

## What changed

- Added reserve-aware compaction controls in [crates/runtime/src/config.rs](d:\新建程序项目\opencowork\crates\runtime\src\config.rs) and [crates/runtime/src/context.rs](d:\新建程序项目\opencowork\crates\runtime\src\context.rs). `context.compactReserveTokens` now lets OpenCoWork begin compaction before the full prompt budget is exhausted.
- Added request-time tool-result budgeting in [crates/runtime/src/compact.rs](d:\新建程序项目\opencowork\crates\runtime\src\compact.rs) and [crates/runtime/src/conversation.rs](d:\新建程序项目\opencowork\crates\runtime\src\conversation.rs). Large file reads, shell output, and oversized match arrays are trimmed before provider submission instead of being passed through unchanged.
- Kept stored session history intact while budgeting provider-bound messages. The runtime now budgets tool results on the request path, not by mutating the persisted session transcript.
- Added same-turn prompt injection for explicit `Skill` tool loads in [crates/runtime/src/prompt.rs](d:\新建程序项目\opencowork\crates\runtime\src\prompt.rs) and [crates/opencowork-cli/src/main.rs](d:\新建程序项目\opencowork\crates\opencowork-cli\src\main.rs). A loaded skill no longer has to rely only on a large raw tool-result payload to influence the next model call inside the same turn.

## Why this matters

- Compaction decisions are less brittle. The runtime no longer has to wait until it is almost fully out of prompt budget before summarizing earlier context.
- Tool output is treated as low-value bulk data unless it remains relevant after trimming. Path metadata, success state, and output previews are preserved; raw bulk payloads are no longer all-or-nothing.
- Skill instructions now behave more like first-class prompt material and less like accidental JSON cargo in the transcript.

## Current limits

- This is still not the full multi-stage pipeline described in some Claude Code writeups. OpenCoWork now has reserve-aware compaction and request-time tool-result budgeting, but it still does not implement a separate named `Snip`, `microCompact`, or `contextCollapse` stage.
- Tool-result budgeting currently focuses on generic large payload handling. It is not yet provider-native token accounting, and it is not yet specialized per tool family beyond common high-volume fields such as `content`, `instruction`, `stdout`, `stderr`, and `matches`.
- Dynamic mid-turn prompt augmentation still centers on skills. LSP-driven context injections, memory recalls, and snippet-level semantic context are still future work.

## Validation

- `cargo fmt`
- `cargo test --target-dir "$env:TEMP\opencowork-target"`
- `cargo clippy --all-targets --all-features --target-dir "$env:TEMP\opencowork-target" -- -D warnings`
