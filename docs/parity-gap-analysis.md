# Parity Gap Analysis

## Canonical reference order

OpenCoWork should keep using these sources in this order:

1. `claude-code-best/claude-code`
2. `ChinaSiro/claude-code-sourcemap`
3. local [`.tmp/claw-code-main`](d:\新建程序项目\opencowork\.tmp\claw-code-main)

The first source is the primary behavioral reference. The second is a lookup source for recovered detail. The local `.tmp` mirror is still useful for Rust-oriented architectural structure, but it is not the parity authority.

## What has already been aligned

- Deferred tool exposure exists and keeps `Skill` out of the default manifest until `ToolSearch` activates it.
- Deferred tool exposure now also covers `LspContext`, which starts the copied LSP service only when the tool is actually used.
- Prompt assembly already layers system instructions, discovered instruction files, compact summaries, and active skill instructions.
- Compaction preserves prior compacted summaries instead of overwriting them.
- Skill loading now covers both `skills/` and legacy `commands/` directory styles, including `.claude`, `.opencowork`, and `.codex` roots.
- Skill metadata now preserves more of the reference frontmatter surface: `when_to_use`, `allowed_tools`, `argument_hint`, `context`, `version`, `agent`, `model`, and `effort`.
- Conditional skill activation now exists for `paths`-scoped skills, using previously touched file paths from session history.
- Conditional skills can now also enter the same turn after file tools run, through runtime prompt augmentation.
- Explicit `Skill` tool loads can now also enter the same turn through runtime prompt augmentation instead of depending only on raw tool-result JSON.
- LSP context can now also enter the same turn through the same prompt augmentation path after `LspContext` runs.
- Request-time tool-result budgeting now trims oversized tool payloads before provider submission while preserving stored session history.
- Request-time tool-result budgeting now also persists oversized payloads to disk and swaps in preview records, closer to the reference `toolResultStorage.ts` flow.
- Context compaction now has a reserve-aware trigger instead of waiting until the entire prompt budget is exhausted.
- Context-window math now honors the reference-style effective-window and autocompact override semantics from the reconstructed repos.
- Skill availability listing now uses the reference-style 1% context budget with the 8K fallback.
- Tool search now understands the reference-style `tst / tst-auto / standard` modes and the 10% auto-threshold heuristic.
- The context pipeline now includes a `snip`-style pre-pass before collapse/full compaction.
- The context pipeline now also includes a `microcompact` pass that clears older compactable tool results before full compaction.
- Persisted compaction now archives removed messages and keeps collapse metadata on the session object across later turns and resume flows.
- A UI-agnostic app/event layer now exists so turn execution is no longer CLI-only.
- Hooks now have a broader event surface and richer config objects than the original pre/post string lists.
- The runtime now has a first real three-layer memory structure:
  - instruction memory with `.claude/CLAUDE.md` and `.claude/rules/**/*.md`
  - current-session memory
  - query-time relevant memory recall
- Nested instruction-memory discovery now reacts to touched files under subdirectories instead of only the current working-directory ancestor chain.
- Session memory now updates through a constrained `edit_file`-only provider-backed nested runtime instead of a free-form text fallback path.
- Team-memory sync now has an initial-pull + delta-push watcher/service with explicit write notifications from the file tools.

## Proven gaps against the reference repos

### High priority

- `contextCollapse` is only partially comparable because the reconstructed reference repos currently expose stubbed `contextCollapse` service modules. OpenCoWork now keeps persisted archive/commit metadata, but not a richer projected archived view for future UIs.
- Hooks are still simpler than the reference repos. The reference supports more hook transports such as prompt, HTTP, and agent-driven hooks, plus a richer matcher tree.
- The task, todo, and team-oriented tool surface from the reconstructed Claude Code repos does not exist yet in OpenCoWork.
- Relevant-memory recall still lacks the reference repos' dedicated side-query route and surfaced-memory throttle behavior.

### Medium priority

- The new `snip` stage is intentionally simpler than the reference repos. It trims an old prefix safely before compaction, but it does not yet emit boundary metadata or replay/persistence semantics like the source-mapped implementation.
- Skill loading does not yet honor the full reference behavior around hidden skills, user-invocable flags, shell execution controls, or per-skill hook payloads.
- Tool search is still smaller than the reference implementation. The new mode logic is aligned, but deferred MCP discovery, discovered-tool deltas, and post-compaction reconstruction are still missing.
- Prompt composition still relies on heuristic token estimates rather than provider-native token accounting.
- Team-memory sync still lacks the reference repos' first-party OAuth-specific auth path, structured 413 entry-limit handling, and some watcher suppression edge cases.
- Mid-turn dynamic context is still narrower than the reference ideas. The augmentation path now covers explicit skills, conditional skills, and `LspContext`, but not yet same-turn relevant-memory recalls or richer semantic snippet selection.

### Intentionally deferred

- Terminal-only rendering ideas such as advanced double-buffer screen management, DECSTBM-based scrolling, or Yoga-style layout should not drive current architecture work because OpenCoWork is being prepared for a future web or desktop shell.

## Current porting rule

- If the reconstructed Claude Code repositories clearly implement a core runtime, tool, skill, hook, or context behavior, OpenCoWork should prefer copying that behavior.
- If a behavior only appears in archived source-map material, old notes, or terminal-only implementation folklore, it should not be prioritized over the current web or EXE target direction.
