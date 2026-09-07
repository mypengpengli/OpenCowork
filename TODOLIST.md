# OpenCoWork Todo

## Current state

- Primary branch and product surface: `ui-shell-session-naming-slash-polish`
- Primary host: `opencowork-desktop` + `opencowork-shell`
- Runtime direction: local-first Rust agent runtime with session persistence, compaction, memory, tools, skills, MCP, and provider-backed turns
- Startup entrypoint for Windows users: `Start-OpenClaw.bat`

## Optimization status and next steps

### Completed: startup (2026-09-06)

- [x] Run existing binaries on daily startup; rebuild explicitly with `Build-OpenCowork.bat` or `Start-OpenClaw.bat --build`.
- [x] Share the Windows build cache across launcher and documented commands; check which checkout produced the binaries.
- [x] Open the desktop after a lightweight `/api/health` check, without waiting for history, skills, MCP, or team-memory initialization.
- [x] Remove runtime/MCP discovery from bootstrap and load team-memory sync through an independent request.
- [x] Read each session once when building history summaries; perform bootstrap disk scans off the async executor.
- [x] Validate with 64 unit tests and isolated startup/launcher checks. Observed service readiness: 507 ms; bootstrap with 30 fixture sessions: 9 ms. These are local service measurements, not full desktop-window timings or a before/after benchmark.

### Existing requirements still open

These were already requested in the priorities below; the startup work does not complete them:

- [ ] Simplify history, chat metadata, and settings; improve code/tool block interactions.
- [ ] Explain why instructions, memories, skills, and deferred tools enter a turn.
- [ ] Separate relevant-memory selection from the main provider request path; add recall throttling and automatic durable-memory extraction.
- [ ] Improve team-memory authentication, conflict handling, and large-payload behavior.
- [ ] Package a runnable desktop release and reduce first-run provider/MCP setup friction.

### Priorities after reviewing Z Code

Reference assumption: ZCODE means Z.ai's Z Code. This is a review of public product documentation, not its implementation or a comparative performance benchmark. P0 is complete as of 2026-09-07; P1/P2 remain proposed work.

| Priority | Proposed improvement | Evidence in this project / acceptance criterion |
| --- | --- | --- |
| P0 complete | Real streaming in the desktop chat | NDJSON forwards text deltas and tool events immediately. The UI renders incoming events without artificial replay delays; the parent saves the final session atomically. |
| P0 complete | Stop a running turn and enforce tool timeouts | Stop/disconnect cancels the worker and its owned subprocesses, preserving received text and pairing unfinished tool calls. Builtin shell commands default to a 120-second timeout, configurable up to 600 seconds. Windows process termination is integration-tested. |
| P1 | Review files and changes in the workspace | Borrow the file-tree, Git-status, and add-file-to-chat workflow from [Z Code task/file management](https://zcode.z.ai/cn/docs/task-management). Start with a changed-file list and read-only diff; make applying or reverting changes explicit. |
| P1 | Explicit context references and diagnostics | Borrow file/session references from [Z Code Agent](https://zcode.z.ai/cn/docs/agents). Show exactly which files or previous sessions are attached, permit removal, and report truncation and token budgets. Build on the existing slash menu and context diagnostics. |
| P1 | Optional background memory extraction | [Z Code Memory](https://zcode.z.ai/cn/docs/memory) describes extraction after successful turns. Add a separate budgeted background route and recall limits while retaining this project's inspect/edit/delete memory UI. Do not put extra extraction requests on the main response path. |
| P1 | Prebuilt distribution and startup feedback | [Z Code installation](https://zcode.z.ai/cn/docs/install) provides desktop installers. Ship prebuilt shell/desktop artifacts together, check WebView2 availability, and surface startup failures. The current fast launcher still needs Rust/MSVC for the first source build. |
| P2 | Visible progress for long tasks | Use [Z Code goal mode](https://zcode.z.ai/cn/docs/goal) as a product reference. Define task steps, progress, completion checks, interruption and recovery before adding more agent orchestration. |

P0 validation: 82 Rust unit tests plus incremental UTF-8 stream tests and loopback integration checks for early output, session conflicts, cancellation, provider errors, descendant termination, and tool timeout recovery. Unix process cleanup is implemented but has not been exercised on this Windows host. The next proposed work is workspace file/change review. Do not replace the Rust runtime merely to imitate another product's UI.

## Current priorities

### Shell and desktop

- Keep polishing the shell UI without changing the backend turn loop unless necessary
- Continue simplifying dense views such as history cards, chat meta, and settings panels
- Improve code/tool block readability and interaction in the shell
- Keep slash command discovery, session naming, and settings flows smooth

### Runtime and context

- Continue improving memory, context, and session behavior against the reference repos
- Keep `ToolSearch`, compaction, session memory, relevant memory, and team memory behavior aligned with the chosen references
- Add better diagnostics for why context, skills, and deferred tools were included in a turn

### Memory parity

- Done: surface the three-layer memory system in the shell so project memory, team memory, and current session memory are inspectable and editable
- Next: split relevant-memory selection into a stricter side-query route instead of piggybacking on the main provider path
- Next: add reference-style surfaced-memory throttling so too many recalled notes cannot accumulate across turns
- Next: add auto-memory distillation / log extraction instead of relying only on session-memory refresh
- Next: keep tightening team-memory sync around richer auth, conflict handling, and large-payload edge cases

### Packaging

- Keep the double-click Windows launcher working from any repo location
- Make desktop/web startup more robust on fresh machines
- Reduce first-run friction in provider setup, skills, MCP, and permissions

## Open items

- Decide whether to add `Start-OpenClaw-Web.bat` and `Stop-OpenClaw.bat`
- Decide whether to ship a release-oriented packaged build instead of build-on-launch
- Decide how much of the current shell should later become a true desktop product surface

## Keep locally, not as repo docs

- Reference notes for memory, parity gaps, source analysis, and architecture are kept under `.tmp/dev-notes/`
- Historical phase reports are intentionally removed from the repo because they are not runtime dependencies

## Quality bar

- No UI-only change should silently break runtime behavior
- No runtime change should be merged just to support shell polish unless it has clear product value
- Keep startup simple for non-technical users
- Keep permission behavior explicit
- Keep the repo clean: retain active guidance, remove stale process notes
