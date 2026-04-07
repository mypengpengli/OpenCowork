# OpenCoWork Architecture Study Report

## Scope

This report captures the system ideas studied from `claw-code` and the way they are being translated into OpenCoWork. The objective is not to mimic repository layout blindly. The objective is to preserve the architectural strengths:

- clear runtime state
- explicit orchestration boundaries
- extension surfaces that do not leak into the core loop
- context control under real budget pressure

## 1. Core design direction

The old project was a thin frontend. That structure could not absorb the runtime capabilities you want. The new repository is therefore organized around control planes rather than UI surfaces:

- `runtime` owns session state, permissions, compaction, context preparation, prompt assembly, and turn execution
- `tools` owns builtin execution primitives and global tool registration
- `plugins` owns local executable extensions and hook aggregation
- `skills` owns instruction-level workflow discovery
- `mcp` owns namespacing and external tool-surface modeling
- `agents` owns role definitions and task-to-agent planning
- `commands` owns slash command parsing and local control verbs
- `opencowork-cli` is only the composition shell

This is the right split because it prevents the runtime loop from becoming a monolith.

## 2. Main loop and prompt orchestration

The main loop pattern worth keeping from `claw-code` is:

1. prepare session state
2. assemble prompt context
3. request assistant output
4. collect tool calls
5. authorize tool execution
6. run hooks
7. execute tools
8. append tool results back into the same conversation
9. repeat until there are no more pending tool uses

OpenCoWork now reflects that split in code:

- turn loop: `crates/runtime/src/conversation.rs`
- prompt layers: `crates/runtime/src/prompt.rs`
- context selection and compaction preparation: `crates/runtime/src/context.rs`

Optimization applied:

- prompt composition is not hardcoded inside the turn loop
- context optimization can compact history before prompt assembly
- instruction sources are priority-ranked so high-value memory wins when budget is tight
- instruction loading is now explicit and configurable through `context.instructionFiles`
- instruction discovery now walks the ancestor chain and deduplicates identical content so nested workspaces inherit root guidance without repeated prompt spam
- previously loaded skills can now flow back into prompt assembly as high-priority instruction layers
- context budgets are no longer implicit constants; they can be tuned through `context.*`

## 3. Tool system

The important lesson from `claw-code` is that tools should look uniform to the model even if they come from different origins.

OpenCoWork now follows that rule:

- builtin tools live in `crates/tools/src/lib.rs`
- plugin tools are merged into the same registry
- MCP naming is normalized into stable prefixes via `crates/mcp/src/lib.rs`
- required permission mode is attached to the tool surface, not bolted on later

Optimization applied:

- one registry path for builtin and plugin tools
- permission requirements travel with tool definitions
- MCP tool names are namespaced deterministically instead of relying on display names
- deferred builtin tools can now stay out of the default manifest and be activated through `ToolSearch`, which is closer to the reference project's startup behavior

## 4. Multi-agent system

The key system lesson is that multi-agent architecture should start with planning and assignment, not with uncontrolled spawning. If routing is implicit, debugging becomes impossible.

OpenCoWork therefore models multi-agent work as:

- agent profile
- capability set
- task requirement set
- scheduling policy
- assignment output

This now lives in `crates/agents/src/lib.rs`.

Optimization applied:

- assignment is deterministic and testable
- specialized roles are preferred by explicit capability matching
- concurrency ceilings are part of policy, not ad hoc runtime behavior

What is still missing:

- persistent background worker pools
- retry and escalation policy

## 5. Hook system

Hooks are useful only when they remain legible. The good pattern is:

- run hooks at clear boundaries
- expose enough environment context for policies and auditing
- let hooks deny execution when needed
- merge hook feedback into the runtime transcript

OpenCoWork now has pre-tool and post-tool hooks in `crates/runtime/src/hooks.rs`, and the turn loop consumes those results in `crates/runtime/src/conversation.rs`.

Optimization applied:

- hook failures can deny execution
- hook output is preserved instead of disappearing into stderr
- hooks are built as runtime policy surfaces, not plugin-only side effects

## 6. Skill / Plugin / MCP ecosystem

These three are not the same thing and should not be collapsed:

- skills: instruction and workflow memory
- plugins: local code that contributes hooks and executable tools
- MCP: remote or proxied capability surfaces

OpenCoWork now models them separately:

- skills: `crates/skills/src/lib.rs`
- plugins: `crates/plugins/src/lib.rs`
- MCP: `crates/mcp/src/lib.rs`

Optimization applied:

- each ecosystem layer has a distinct ownership boundary
- the runtime can combine them without confusing their responsibilities
- future auditing becomes easier because extension origin stays visible
- MCP tools can now be declared, discovered, namespaced, and merged into the same registry model
- MCP resources can now be discovered and read separately from tool execution
- MCP auth can now be modeled as env, file, or saved local OAuth-style credentials

## 7. Context compression and optimization

This is one of the most valuable ideas in the source project. Long-running coding sessions fail if context control is weak.

OpenCoWork now has two separate stages:

- compaction: summarize older conversation state into a structured rollover prefix
- context selection: choose recent messages and high-priority instructions under a budget

Relevant files:

- compaction: `crates/runtime/src/compact.rs`
- context selection: `crates/runtime/src/context.rs`
- prompt assembly: `crates/runtime/src/prompt.rs`

Optimization applied:

- compaction is a first-class runtime action
- recent messages and historical summary are treated differently
- instruction memory competes on priority instead of first-come-first-served
- repeated compaction now preserves previously summarized context and merges it with newly summarized work instead of discarding older rollovers
- timeout and load knobs are now surfaced instead of buried in code defaults

What still needs deeper work:

- provider-native token counting
- retrieval-aware file/context scoring
- richer summary schemas for ongoing implementation threads

## 8. Current assessment

OpenCoWork is now structurally aligned with the right runtime model. The repository is no longer blocked by its old frontend shape.

The current implementation should be viewed as a strong systems build, not as final product parity. The biggest missing pieces are now product-depth issues, not missing control planes: terminal rendering is still intentionally plain, MCP auth does not yet include browser-driven OAuth login, and the worker service is persistent but not yet a fully managed packaged daemon.

## 9. Next optimization targets

- improve streamed terminal rendering quality while keeping the current observer-based event path
- add persistent session switching and richer resume metadata
- add MCP template flows and browser-based OAuth on top of the transport layer
- extend the current skill prompt reinjection into typed skill metadata and stronger invocation contracts
- add agent handoff transcripts and durable task state
- add richer slash commands for inspection and debugging
