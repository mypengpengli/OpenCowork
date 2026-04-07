# Phase 12 Report

This phase pushed OpenCoWork closer to the reference project's stronger runtime behavior in three areas: large-window context policy, lazy semantic tooling, and a UI-agnostic execution layer.

## What changed

- Raised the default context policy in [crates/runtime/src/config.rs](d:\新建程序项目\opencowork\crates\runtime\src\config.rs) and [crates/runtime/src/context.rs](d:\新建程序项目\opencowork\crates\runtime\src\context.rs) from a small fixed budget to a 200K-class model policy. The defaults now assume `maxPromptTokens = 180000` with `compactReserveTokens = 20000` instead of the older `16000 / 2000`.
- Added a finer context-preparation pipeline in [crates/runtime/src/compact.rs](d:\新建程序项目\opencowork\crates\runtime\src\compact.rs) and [crates/runtime/src/context.rs](d:\新建程序项目\opencowork\crates\runtime\src\context.rs). OpenCoWork now budgets oversized tool results, collapses older long text messages, and only then decides whether full session compaction is required.
- Ported the local reference repository's LSP subsystem into [crates/lsp/src/lib.rs](d:\新建程序项目\opencowork\crates\lsp\src\lib.rs), [crates/lsp/src/manager.rs](d:\新建程序项目\opencowork\crates\lsp\src\manager.rs), and [crates/lsp/src/client.rs](d:\新建程序项目\opencowork\crates\lsp\src\client.rs).
- Added `lspServers.*` config parsing in [crates/runtime/src/config.rs](d:\新建程序项目\opencowork\crates\runtime\src\config.rs) and exposed deferred `LspContext` activation in [crates/tools/src/lib.rs](d:\新建程序项目\opencowork\crates\tools\src\lib.rs). The LSP process is not started during CLI boot; it is only started on first actual tool use.
- Reused the mid-turn prompt augmentation path so `LspContext` can affect the same turn after it runs, in [crates/opencowork-cli/src/main.rs](d:\新建程序项目\opencowork\crates\opencowork-cli\src\main.rs) and [crates/app/src/lib.rs](d:\新建程序项目\opencowork\crates\app\src\lib.rs).
- Added a new UI-agnostic composition layer in [crates/app/src/lib.rs](d:\新建程序项目\opencowork\crates\app\src\lib.rs). It loads the runtime environment, prepares prompt plans, runs turns, and emits structured `AppEvent` values that future web or EXE shells can consume without talking directly to CLI internals.
- Routed CLI `prompt`, `resume`, and `prompt-plan` through the new app layer in [crates/opencowork-cli/src/main.rs](d:\新建程序项目\opencowork\crates\opencowork-cli\src\main.rs) and [crates/opencowork-cli/src/render.rs](d:\新建程序项目\opencowork\crates\opencowork-cli\src\render.rs).

## Why this matters

- OpenCoWork no longer behaves as if a modern coding model only had a tiny context window.
- The context pipeline now has a clearer ordering closer to the reference ideas: trim bulky payloads first, keep detail when possible, and compact only when necessary.
- LSP is no longer just a future note. There is now a real lazy semantic service with diagnostics, definitions, references, and prompt-ready enrichment.
- Future UI work now has a proper seam. The CLI is still a host, but it is no longer the only place that knows how to execute a turn.

## Remaining gaps

- The context pipeline is still not the full named `Snip / microCompact / contextCollapse / autoCompact` matrix from external writeups; it is a practical staged approximation.
- `LspContext` is the first lazy semantic tool, not the full LSP tool family.
- The new app layer is intentionally narrow. It covers turn execution and event streaming, but not yet full session switching, background workers, or a visual shell.

## Validation

- `cargo test --target-dir "$env:TEMP\opencowork-target"`
- `cargo clippy --all-targets --all-features --target-dir "$env:TEMP\opencowork-target" -- -D warnings`
- `cargo run --target-dir "$env:TEMP\opencowork-target" -q -p opencowork-cli -- provider`
- `cargo run --target-dir "$env:TEMP\opencowork-target" -q -p opencowork-cli -- prompt-plan`
- `cargo run --target-dir "$env:TEMP\opencowork-target" -q -p opencowork-cli -- tool-manifest`
