# Phase 18 Report

## Goal

Continue closing the gap with the reference repositories on the three-layer memory system without introducing OpenCoWork-only memory mechanics.

## What changed

- Switched session-memory refresh from the local deterministic turn-end path to a provider-backed background refresh path in both the reusable app host and the CLI host.
- Kept the deterministic renderer only as fallback when provider initialization or provider extraction fails.
- Split session-memory state so extraction cadence and compact boundaries no longer share one counter:
  - `last_triggered_message_count`
  - `last_summarized_message_count`
- Added persisted session-memory state sidecars under the session-memory directory so background extraction can update state that later turns hydrate back into the session.
- Added a short wait path before prompt preparation so an in-flight session-memory extraction can complete before the next turn reads current session memory.

## Main files

- [memory.rs](d:\新建程序项目\opencowork\crates\runtime\src\memory.rs)
- [session.rs](d:\新建程序项目\opencowork\crates\runtime\src\session.rs)
- [session_memory.rs](d:\新建程序项目\opencowork\crates\runtime\src\session_memory.rs)
- [lib.rs](d:\新建程序项目\opencowork\crates\app\src\lib.rs)
- [main.rs](d:\新建程序项目\opencowork\crates\opencowork-cli\src\main.rs)

## Reference alignment

This phase moved OpenCoWork closer to these reference behaviors:

- background session-memory extraction
- separation between extraction-trigger state and summary/compaction boundary state
- wait-before-consume handling for in-flight session-memory extraction

It still does not fully match the reference repositories in two places:

- extraction is provider-backed, but not yet a stricter forked-agent flow with constrained file editing
- team-memory sync/watcher and auto-memory distillation are still missing

## Verification

- `cargo fmt`
- `cargo test --target-dir "$env:TEMP\\opencowork-target"`
- `cargo clippy --all-targets --all-features --target-dir "$env:TEMP\\opencowork-target" -- -D warnings`
