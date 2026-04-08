# OpenCoWork Rebuild Todo

## Completed in current rebuild

- Replaced the previous page-only project with a Rust-first local runtime workspace.
- Added session persistence, compaction, prompt composition, and context selection.
- Added plugin installation and persisted plugin enabled state.
- Added MCP server loading, MCP tool definitions, and unified MCP tool naming.
- Added MCP stdio and HTTP transport execution plus live tool discovery.
- Added MCP SSE and WebSocket remote transport paths.
- Added multi-agent planning plus persisted agent handoff records and handoff state transitions.
- Added backward-compatible handoff loading so older persisted task files still work after schema upgrades.
- Added a provider bridge layer so runtime-side API clients can be backed by provider adapters.
- Added live OpenAI-compatible provider execution with real CLI prompt and resume flows.
- Added provider streaming normalization from SSE into runtime events.
- Added a CLI handoff worker loop for sustained delegated task consumption.
- Added realtime CLI rendering by streaming provider events through the runtime loop instead of printing only after turn completion.
- Added configurable context load rules for recent-message preservation, token budgets, and `context.instructionFiles`.
- Added MCP `resources/list` and `resources/read` support.
- Added MCP auth models plus saved local token storage for OAuth-style credentials.
- Added persistent worker-service `start/run/status/stop` flows with pid, heartbeat, stale detection, and stop flags.
- Added provider and MCP timeout knobs so long-running loops are not locked to hardcoded defaults.
- Added `ToolSearch` plus deferred builtin tool exposure so specialized tools do not need to be present in the initial manifest.
- Added `Skill` loading through the unified tool registry and prompt-side reinjection of active skill instructions from prior tool results.
- Added ancestor-chain instruction discovery and content deduplication for `CLAUDE` / `.opencowork` / `.codex` / `CLAW` style instruction files.
- Upgraded session compaction so repeated compaction preserves previously summarized context instead of overwriting it.
- Added skill discovery parity work for `.claude`, `.opencowork`, and `.codex` `skills/` plus legacy `commands/` roots.
- Added richer skill metadata loading for `when_to_use`, `argument_hint`, `allowed_tools`, `context`, `version`, `agent`, `model`, and `effort`.
- Added conditional skill activation using `paths` frontmatter and previously touched file paths from session history.
- Added a richer hook event/config skeleton, including session, prompt-submit, permission-denied, failure, instruction-load, and file-change hooks.
- Fixed hook assembly so runtime config hooks and plugin hooks are merged instead of only plugin hooks executing.
- Added runtime prompt augmentation so conditionally activated skills can enter the same turn after file tools run.
- Added same-turn prompt injection for explicit `Skill` tool loads instead of relying only on raw tool-result payloads.
- Added reserve-aware compaction thresholds through `context.compactReserveTokens`.
- Raised default context budgets to a 200K-class model policy with a 20K reserve instead of a 16K fixed cap.
- Added staged context preparation so tool-result budgeting and older-message collapse run before full session compaction.
- Added request-time tool-result budgeting so oversized file/bash/search payloads can be trimmed before provider submission without mutating stored session history.
- Added a lazily started LSP subsystem plus deferred `LspContext` tool activation.
- Added same-turn LSP prompt injection so semantic context can affect the current turn after the tool runs.
- Added a UI-agnostic `opencowork-app` layer so prompt execution and event streaming are not CLI-only concerns anymore.
- Aligned context-window math more closely with the reference repos by honoring `CLAUDE_CODE_AUTO_COMPACT_WINDOW` and `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`.
- Added `snip`-style history trimming ahead of collapse/full compaction so oversized old prefixes can drop out before summary compaction kicks in.
- Added reference-style `ENABLE_TOOL_SEARCH` mode handling (`tst`, `tst-auto`, `standard`) so deferred builtin tools can inline or defer based on the same mode semantics as the reference repos.
- Added `ToolSearch` diagnostics for deferred-description size and auto-threshold size so the current manifest policy is inspectable.
- Added reference-style three-layer memory groundwork: project `MEMORY.md`, current-session memory, and query-time relevant-memory recall.
- Added `.claude/CLAUDE.md` plus `.claude/rules/**/*.md` discovery into the instruction-memory surface.
- Added touched-path-driven nested instruction-memory discovery for subdirectories below the current workspace root.
- Moved touched-path extraction into the runtime layer so CLI/app do not carry duplicated path parsing logic.
- Added provider-backed relevant-memory selection with heuristic fallback.
- Added reference-style session-memory thresholds (`10k / 5k / 3`) and stale extraction tracking.
- Added team-memory entrypoint loading from `memory/team/MEMORY.md`.
- Added session-memory-first compaction with preserved-tail thresholds, tool-pair preservation, and oversized-section truncation.
- Added reference-style team-memory write guarding and secret scanning in the file tools.
- Added background session-memory scheduling plus disk hydration before the next turn's context preparation.
- Switched session-memory extraction over to a provider-backed background path with deterministic fallback.
- Split session-memory trigger state from compaction-boundary state so extraction cadence and compact boundaries no longer share one field.
- Added persisted session-memory state sidecars and a wait-before-hydrate path so in-flight extraction can complete before the next prompt plan is assembled.
- Added a `microcompact` pass that clears older compactable tool results before full compaction while keeping recent tool outputs intact.
- Added persisted context-collapse archive tracking so removed messages survive as archived state across later turns and resume flows.
- Switched provider-backed session-memory extraction to a constrained `edit_file` tool flow running in an isolated nested runtime.
- Added team-memory sync with initial pull, delta push, git remote repo detection, write notifications, background watcher/poller, and CLI status/pull/push controls.
- Added a web shell refinement pass with auto-named sessions, slash command palette, polished top popovers, and improved send/pending UX.
- Reduced shell UI noise by moving session overview and runtime helper into top popovers instead of keeping them inline in the chat surface.
- Improved shell message/history presentation with derived session titles, previews, status chips, and lighter conversation actions.
- Refined shell code/tool blocks and settings forms toward a cleaner IDE-like presentation with better spacing, focus feedback, and block viewing.
- Shell polish checklist: completed a more IDE-like treatment for code-ish blocks and the block viewer, including a structured viewer with line numbers.
- Shell polish checklist: completed a lower-density settings pass with narrower copy widths, roomier spacing, and single-column settings cards where dense side-by-side layouts were noisy.
- Shell polish checklist: completed a richer session/history status pass with message-count pills, quieter preview clamping, and more polished state chips.

## Priority 0

- Replace the previous page-only project completely with a local-first runtime workspace.
- Stabilize the Rust workspace so the runtime, tools, plugins, skills, MCP, and agent layers compile together.
- Keep the architecture inspired by `claw-code`, but keep all code, naming, and product decisions native to OpenCoWork.

## Priority 1

- Strengthen prompt composition with file relevance scoring and provider-native token budgeting.
- Expand session persistence into named session switching and richer resume metadata.
- Improve context optimization so instruction memory, summary context, recent turns, active skills, and tool-result budgets are selected with better heuristics.
- Extend the context-collapse archive into a richer projected archived view if a future UI needs it.
- Upgrade relevant-memory recall from heuristic matching to a selector closer to the reference repositories.
- Replace the simplified provider-backed relevant-memory selector with a dedicated side-query path closer to the reference implementation.
- Add richer terminal rendering beyond the current plain streamed markdown passthrough.
- Surface same-turn conditional skill and LSP activation in diagnostics and future UI event streams, not only inside runtime state.

## Priority 2

- Build a stronger multi-agent scheduler with role selection, task routing, retry policy, and richer handoff state.
- Add more LSP-backed tools beyond `LspContext`, such as targeted diagnostics and symbol navigation commands.
- Carry deferred-tool discovery state across compaction boundaries so `ToolSearch` can reconstruct previously discovered tools more like the reference repos.
- Add deeper MCP lifecycle state management, resources/templates APIs, and browser-based OAuth flows.
- Bring hooks closer to the reference event/type matrix with matcher trees, richer transports, and non-command hook types.
- Reuse the new runtime prompt augmentation mechanism for LSP/context snippets and future memory injections.

## Priority 3

- Add structured task planning state similar to an integrated todo/task graph.
- Promote the current persistent worker service into a packaged desktop/background runtime service.
- Add deeper prompt analytics and compaction heuristics driven by real token accounting.
- Add richer slash commands for sessions, agents, skills, config inspection, and diagnostics.
- Add team memory and project-memory maintenance flows closer to the reconstructed Claude Code repos.

## Quality bar

- Every major subsystem should have tests for success and failure paths.
- Session state and plugin state must remain portable JSON, not hidden binary state.
- The runtime must keep permission decisions explicit and auditable.
- Reports and docs must reflect the actual implementation state, not aspirational marketing copy.
