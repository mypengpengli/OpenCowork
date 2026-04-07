# CLAUDE.md

This repository is no longer a frontend-only Vite app. It is a Rust-first local agent runtime called OpenCoWork.

## Project direction

OpenCoWork is being rebuilt around these runtime concerns:

- session state and resume-safe history
- prompt composition and context budgeting
- streamed terminal rendering
- tool execution with permissions
- hooks, plugins, skills, and MCP extension surfaces
- multi-agent planning and task routing

## Workspace layout

- `crates/runtime`: turn loop, prompt layers, context optimization, session store, permissions, hooks
- `crates/app`: UI-agnostic app/event layer for future web or desktop shells
- `crates/lsp`: lazy LSP manager and semantic context enrichment
- `crates/tools`: builtin tools and global tool registry
- `crates/plugins`: plugin manifest, installation, enabled state, plugin tools
- `crates/skills`: skill discovery
- `crates/mcp`: MCP naming, resources, auth, and transport helpers
- `crates/agents`: agent role and scheduling model
- `crates/commands`: slash command parsing and local control commands
- `crates/opencowork-cli`: composition shell for the whole workspace

## Commands

```powershell
cargo fmt
cargo test --target-dir "$env:TEMP\opencowork-target"
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- help
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- prompt-plan
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- provider
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- mcp
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- mcp auth
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- handoffs create
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- handoffs worker nobody --max-jobs 1
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- handoffs service status
```

## Notes

- On this machine, provider-side dependencies compile reliably when Cargo uses `--target-dir "$env:TEMP\opencowork-target"`.
- Context loading is configurable through `context.*` settings, especially `instructionFiles`, `preserveRecentMessages`, token budgets, `compactReserveTokens`, and large-message collapse settings.
- Remote MCP auth state is stored under `OPENCOWORK_CONFIG_HOME/mcp-auth`.
- Persistent worker-service state is stored under `OPENCOWORK_CONFIG_HOME/worker-service`.
- Keep new architecture work in Rust unless there is a strong reason to add another runtime.
- Do not reintroduce the removed React/Vite scaffold.
