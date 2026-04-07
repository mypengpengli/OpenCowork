# Phase 3 Report

## Goal

The third rebuild phase closes three important architectural gaps:

- a provider bridge layer so the runtime can be backed by model adapters
- MCP tools merged into the same global registry model as builtin and plugin tools
- persistent agent handoff records so planned delegation survives process boundaries

## What changed

- `crates/api` now exposes provider request/response abstractions and a replay provider client
- `crates/runtime/src/provider_bridge.rs` converts provider events into runtime assistant events
- `crates/mcp` now models tool definitions, permissions, bindings, and static executors
- `crates/runtime/src/config.rs` now loads MCP tool definitions from config
- `crates/tools` now merges builtin, plugin, and MCP tools into one registry
- `crates/agents` now persists handoff records as JSON
- `crates/opencowork-cli` now exposes:
  - `mcp`
  - `handoffs [create]`
  - `provider`

## Design impact

This phase matters because OpenCoWork now has the correct seams for live orchestration:

- provider layer and runtime layer are no longer fused
- external tool surfaces are no longer second-class compared to builtin tools
- multi-agent planning now leaves durable records instead of disappearing after one command

## Validation

Validated with:

```powershell
cargo test --target-dir C:\t
cargo run --target-dir C:\t -p opencowork-cli -- mcp
cargo run --target-dir C:\t -p opencowork-cli -- handoffs create
cargo run --target-dir C:\t -p opencowork-cli -- provider
```

## Remaining gaps

- provider bridge still uses a replay client rather than a live HTTP transport
- MCP tools can be registered and executed through static handlers, but remote transport is not implemented yet
- handoffs are persisted, but no background worker runtime consumes them yet
- the turn loop still defaults tool-call permissions conservatively when provider-side tool metadata is absent

## Conclusion

OpenCoWork is now past the point where another rewrite is needed. The remaining work is deeper implementation, not architecture rescue.
