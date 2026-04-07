# OpenCoWork Phase 6 Report

## Scope

Phase 6 focused on the three system gaps that were still materially behind the reference direction:

- realtime streamed terminal rendering
- MCP resources and auth state
- persistent worker-service control

The work in this phase stayed on the same architectural rule as the reference project: do not fork the runtime loop just to improve operator UX. Instead, expose the right mechanism boundaries and let CLI behavior ride on those boundaries.

## 1. Realtime streaming terminal rendering

The key change is that provider events can now be observed while the main runtime loop is still running.

Implemented changes:

- `crates/api/src/lib.rs`
  - added `ProviderClient::stream_execute_with`
- `crates/api/src/openai_compat.rs`
  - OpenAI-compatible SSE parsing now emits `ProviderEvent` values incrementally while still collecting the final event list
- `crates/runtime/src/conversation.rs`
  - added runtime observer support so assistant events and tool results can be mirrored out while keeping the existing turn loop
- `crates/runtime/src/provider_bridge.rs`
  - provider-backed API client now translates provider events incrementally into runtime events
- `crates/opencowork-cli/src/render.rs`
  - added `CliTurnRenderer`, `MarkdownStreamState`, and safe flush logic

Mechanism details:

- markdown output is not printed one byte at a time; it flushes at safe boundaries and also has an eager fallback threshold so long paragraphs do not stall forever
- tool calls are rendered when the provider has finished accumulating their arguments
- tool results are emitted from the same runtime loop after hooks, permissions, and execution complete
- the CLI still uses the same session, permission, and hook path as non-streamed execution

## 2. MCP resources and auth

The transport layer now models more than tools.

Implemented changes:

- `crates/mcp/src/lib.rs`
  - added MCP resource models
  - added auth models for `none`, `bearer-env`, `bearer-file`, and `oauth`
  - added local credential storage
- `crates/mcp/src/transport.rs`
  - added `discover_resources`
  - added `read_resource`
  - added remote auth header injection
  - added per-server timeout handling for HTTP and SSE request builders
- `crates/runtime/src/config.rs`
  - parses `mcpServers.<name>.auth`
  - still accepts `mcpServers.<name>.oauth`
  - parses `mcpServers.<name>.timeoutMs`
- `crates/opencowork-cli/src/main.rs`
  - added `mcp resources`
  - added `mcp resource read <server> <uri>`
  - added `mcp auth`
  - added `mcp auth save <server> <token>`
  - added `mcp auth clear <server>`

Mechanism details:

- auth presence is surfaced explicitly in `mcp` summary output
- OAuth-style credentials are stored locally under config home instead of being buried in ad hoc files
- if a server requires auth and the credential source is missing, discovery/execution now fails loudly instead of silently degrading
- MCP tools and resources remain separate surfaces, which matches the cleaner reference design

## 3. Persistent worker service

The old `handoffs worker` loop was useful for development but not sufficient for packaged software. This phase adds a persistent control surface around it.

Implemented changes:

- `crates/opencowork-cli/src/main.rs`
  - added `handoffs service start <agent>`
  - added `handoffs service run <agent>`
  - added `handoffs service status [agent]`
  - added `handoffs service stop <agent>`
  - added persistent service-state JSON plus log-file paths

Mechanism details:

- service state persists `pid`, `poll_ms`, `heartbeat_ms`, processed count, status, stop flag, and last error
- service state is written under `OPENCOWORK_CONFIG_HOME/worker-service`
- stale detection is heartbeat-based instead of assuming process liveness
- `service run` is the long-lived execution path; `service start` is the spawn wrapper

## 4. Context and timeout knobs

Because the request stressed small mechanism details, Phase 6 also exposed several control knobs that were previously hardcoded:

- `provider.timeoutMs`
- `context.preserveRecentMessages`
- `context.maxPromptTokens`
- `context.maxInstructionTokens`
- `context.instructionFiles`
- `mcpServers.<name>.timeoutMs`

This matters because operator-facing quality depends on these details more than on large feature labels.

## 5. Remaining gaps

Phase 6 closes the previously identified architectural holes, but some depth work is still pending:

- streamed terminal rendering is still intentionally plain and not yet syntax-highlighted
- MCP auth does not yet run a browser-based OAuth login callback flow
- MCP templates/resources subscriptions are still absent
- worker service is persistent, but not yet a formally installed OS service
