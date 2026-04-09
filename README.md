# OpenCoWork

`opencowork` is being rebuilt as a local-first agent runtime instead of the previous frontend-only app.

The new direction follows the strong architectural ideas demonstrated by `claw-code`:

- typed session history with tool-use and tool-result blocks
- runtime-level permission policy
- compact/summary-based context rollover
- plugin registry with persisted enabled state
- prompt composition and context budgeting
- session persistence and resume-safe local state
- agent planning, role division, and scheduling
- skill and MCP extension catalogs
- slash-command oriented CLI surface
- layered settings discovery and merge rules

This repository is a fresh implementation for OpenCoWork. The goal is to keep the architecture quality, not to preserve the old Vite scaffold. The CLI is only the current host shell; the runtime is intended to be wrapped later by a web or desktop UI.

## Workspace layout

```text
.
+-- crates/api            # provider-facing request and stream types
+-- crates/app            # UI-agnostic app runtime and event stream for future web/desktop shells
+-- crates/lsp            # lazy LSP manager and semantic context enrichment
+-- crates/runtime        # session, compaction, config, hooks, permissions, turn loop
+-- crates/tools          # builtin tool registry and file/shell tools
+-- crates/plugins        # plugin manifests, registry, installation, enabled state
+-- crates/agents         # multi-agent role definitions and scheduling
+-- crates/skills         # skill discovery and metadata loading
+-- crates/mcp            # MCP naming, registry, resources, auth, transport
+-- crates/commands       # slash commands and command help
+-- crates/opencowork-cli # executable entrypoint
```

## Current status

The repository is now in the fourteenth rebuild phase. The old frontend scaffold is gone. OpenCoWork now has real execution paths for provider-backed turns, realtime streamed CLI rendering, MCP remote transport plus resources/auth state, session resume, persistent worker-service control, a UI-agnostic app/event layer, a stronger context/tool orchestration core, and a first real version of the reference-style three-layer memory system.

The current repository state also includes:

- provider bridge abstractions plus a live OpenAI-compatible client
- incremental SSE streaming routed into the CLI renderer without forking the main runtime loop
- unified builtin/plugin/MCP tool registration
- MCP `stdio`, `http`, `sse`, and `ws` transport execution paths
- MCP `resources/list` and `resources/read` support
- MCP auth models for `bearer-env`, `bearer-file`, and saved local OAuth-style tokens
- provider and MCP timeout surfaces in config
- context load rules for `preserveRecentMessages`, token budgets, and `context.instructionFiles`
- 200K-class default context budgeting with a 20K reserve instead of the previous small fixed default
- ancestor-chain instruction discovery with content dedupe across `CLAUDE`, `.opencowork`, `.codex`, and imported `CLAW` style files
- `.claude/CLAUDE.md` and `.claude/rules/**/*.md` instruction discovery, closer to the reconstructed Claude Code memory surface
- nested instruction-memory discovery for touched subdirectories instead of only the current working directory chain
- project-scoped `MEMORY.md` loading for durable workspace memory
- current-session memory persistence and reinjection into prompt composition
- query-time relevant-memory recall from the project memory directory
- deferred tool activation through `ToolSearch`, with `Skill` kept out of the initial tool manifest until activated
- deferred `LspContext` activation with a lazily started LSP service copied from the local reference implementation
- prompt-side reinjection of active skill instructions so explicit skill loads can shape the same turn and later turns
- compaction that merges previously compacted context with newly summarized context instead of replacing it
- reserve-aware auto-compaction so compaction can start before the prompt budget is completely exhausted
- reference-style context-window override handling for `CLAUDE_CODE_AUTO_COMPACT_WINDOW` and `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`
- staged context preparation that budgets tool results, collapses older large text messages, and only then decides whether full compaction is necessary
- a `snip` pre-pass before collapse/full compaction so oversized old prefixes can fall out before summary compaction
- request-time tool-result budgeting so very large file/bash/match payloads do not pass through to the provider unchanged
- persisted collapse metadata in saved sessions so compacted spans survive future turns and resume flows as structured state
- skill discovery across `.claude`, `.opencowork`, and `.codex` roots, including legacy `commands/` directories
- richer skill frontmatter loading for `when_to_use`, `allowed-tools`, `argument-hint`, `context`, `agent`, `model`, and `effort`
- conditional skill activation based on previously touched file paths declared through skill `paths` frontmatter
- same-turn conditional skill activation after file tools run, via runtime prompt augmentation instead of waiting for the next turn
- same-turn LSP context injection through the same prompt augmentation path used by skills
- runtime hook events beyond plain pre/post tool strings, including session, prompt-submit, failure, file-change, and instruction-load events
- merged runtime config hooks plus plugin hooks instead of plugin-only hook execution
- persisted agent handoff records with state transitions and backward-compatible loading
- `crates/app` event types and turn execution helpers so CLI is no longer the only composition layer
- reference-style `ToolSearch` mode handling for `tst`, `tst-auto`, and `standard`, plus manifest diagnostics for deferred-tool sizing
- CLI flows for `provider`, `prompt`, `resume`, `mcp auth`, `mcp resources`, `handoffs consume`, `handoffs worker`, and `handoffs service`

## Windows Quick Start

If you just want to double-click and run the project on Windows, use:

```text
Start-OpenClaw.bat
```

The launcher resolves the project root from its own location, so it still works if the repository is moved to a different folder. It will:

- build the desktop shell first
- start `opencowork-desktop` when the desktop host is available
- fall back to the web shell and open `http://127.0.0.1:33211/` if the desktop host cannot be started

### What a new Windows machine needs

For a fresh machine, the project currently needs:

- Rust toolchain (`rustup`, `cargo`, `rustc`)
- Windows MSVC C++ build environment
  install either Visual Studio 2022 Build Tools or Visual Studio with the C++ desktop workload
- Microsoft Edge WebView2 Runtime
  required by the desktop shell hosted through `wry`

Useful notes:

- Node.js is not required for the current Rust shell/desktop startup path.
- Git is only needed to clone/pull the repository, not to run an already-downloaded copy.
- MCP servers or custom tools may need their own runtimes later, depending on what you configure.
- The first build on a new machine will be slower because Cargo needs to compile the workspace.

## Commands

```powershell
cargo fmt
cargo test --target-dir "$env:TEMP\opencowork-target"
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- provider
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- mcp
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- mcp auth
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- mcp resources
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- handoffs create
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- handoffs worker nobody --max-jobs 1
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- handoffs service status
cargo run --target-dir "$env:TEMP\opencowork-target" -p opencowork-cli -- prompt "inspect this workspace"
```

## Config knobs

The current rebuild exposes the small control knobs that matter operationally:

- `provider.timeoutMs`
- `context.preserveRecentMessages`
- `context.maxPromptTokens`
- `context.maxInstructionTokens`
- `context.compactReserveTokens`
- `context.messageCollapseChars`
- `context.toolResultSoftChars`
- `context.toolResultHardChars`
- `context.instructionFiles`
- `lspServers.<name>.command`
- `lspServers.<name>.languages`
- `lspServers.<name>.workspaceRoot`
- `mcpServers.<name>.timeoutMs`
- `mcpServers.<name>.auth`
- `mcpServers.<name>.oauth`

## Working docs

- `TODOLIST.md`
- `docs/phase-2-report.md`
- `docs/phase-3-report.md`
- `docs/phase-4-report.md`
- `docs/phase-5-report.md`
- `docs/phase-6-report.md`
- `docs/phase-7-report.md`
- `docs/phase-8-report.md`
- `docs/phase-9-report.md`
- `docs/phase-10-report.md`
- `docs/phase-11-report.md`
- `docs/phase-12-report.md`
- `docs/phase-13-report.md`
- `docs/phase-14-report.md`
- `docs/reference-source-analysis.md`
- `docs/reference-memory-analysis.md`
- `docs/parity-gap-analysis.md`
- `docs/architecture-study-report.md`
