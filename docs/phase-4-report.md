# Phase 4 Report

## Goal

The fourth rebuild phase moves OpenCoWork from architecture-only progress into real execution:

- live OpenAI-compatible provider requests
- MCP transport-aware execution
- session resume and handoff consume flows

## What changed

- `crates/api` now includes a real OpenAI-compatible HTTP client in `src/openai_compat.rs`
- `crates/runtime` now carries explicit tool manifests through the request path instead of inferring them from transcript history
- `crates/runtime/src/config.rs` now exposes `model` and `provider` settings
- `crates/mcp` now includes `TransportMcpExecutor` with stdio and HTTP JSON-RPC execution plus tool discovery
- `crates/tools` now exports runtime tool definitions and implements the runtime `ToolExecutor` trait directly
- `crates/agents` now persists handoff state transitions: pending, running, completed, failed
- `crates/opencowork-cli` now exposes:
  - `provider`
  - `prompt <text>`
  - `resume <id> <text>`
  - `handoffs consume <agent>`

## Design impact

This phase matters because OpenCoWork no longer stops at scaffolding:

- model requests now follow a provider adapter pattern with real HTTP transport
- MCP is no longer config decoration; it is an executable external tool surface
- agent delegation is no longer write-only metadata; it can now be consumed into real runtime work
- tool permissions now travel with tool definitions across the runtime boundary

## Validation

Validated with:

```powershell
cargo test --target-dir "$env:TEMP\opencowork-target"
cargo run --target-dir "$env:TEMP\opencowork-target" -q -p opencowork-cli -- provider
cargo run --target-dir "$env:TEMP\opencowork-target" -q -p opencowork-cli -- mcp
cargo run --target-dir "$env:TEMP\opencowork-target" -q -p opencowork-cli -- handoffs create
```

## Remaining gaps

- prompt/context heuristics are still approximate and not yet backed by provider-native token accounting
- provider streaming is still collected before the CLI prints final output
- handoff execution is foreground CLI consumption, not a long-running worker service

## Conclusion

OpenCoWork is now running through the same major control planes that made the reference project strong: explicit session state, structured tool orchestration, external capability surfaces, and durable delegation state. The remaining work is parity depth, not another architecture reset.
