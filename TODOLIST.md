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

- [x] Simplify history, chat metadata, and settings; improve code/tool block interactions. Delivered 2026-09-11: fixed composer, grouped tool dock, date-grouped history, secondary actions and usage disclosures, advanced execution settings, keyboard navigation and compact window layout.
- [x] Show included prompt layers, attachment budgets, truncation and context reasons; continue refining deferred-tool explanations.
- [x] Remove extra provider calls for memory selection; add bounded lexical recall, repeated-note throttling and optional background durable-memory extraction.
- [ ] Improve team-memory authentication, conflict handling, and large-payload behavior.
- [x] Add portable shell/desktop packaging, WebView2 checks and visible startup errors.
- [ ] Further reduce first-run provider/MCP setup friction.

### Priorities after reviewing Z Code

Reference assumption: ZCODE means Z.ai's Z Code. This is a review of public product documentation, not its implementation or a comparative performance benchmark. P0 is complete as of 2026-09-07; the P1/P2 baseline below is implemented as of 2026-09-09.

| Priority | Proposed improvement | Evidence in this project / acceptance criterion |
| --- | --- | --- |
| P0 complete | Real streaming in the desktop chat | NDJSON forwards text deltas and tool events immediately. The UI renders incoming events without artificial replay delays; the parent saves the final session atomically. |
| P0 complete | Stop a running turn and enforce tool timeouts | Stop/disconnect cancels the worker and its owned subprocesses, preserving received text and pairing unfinished tool calls. Builtin shell commands default to a 120-second timeout, configurable up to 600 seconds. Windows process termination is integration-tested. |
| P1 complete | Review files and changes in the workspace | File list, Git status, text previews, staged/unstaged read-only diff and add-to-chat. Non-Git folders use a bounded file scan. Versioned stage/unstage and text revert controls were added in the subsequent optimization round. |
| P1 complete | Explicit context references and diagnostics | Removable file/session references, eight-item limit, per-item/total character budgets, truncation and estimated tokens. References persist with user turns; included prompt layers have diagnostics. |
| P1 complete | Optional background memory extraction | Independent worker after successful turns, 768 output tokens, timeout, cooldown and per-project lock. Existing memory inspection/edit/delete remains available; foreground recall is bounded and throttled. |
| P1 complete | Prebuilt distribution and startup feedback | Timestamped portable ZIP with both executables, launcher and hash manifest. WebView2 check, startup dialog and log files; prebuilt runtime needs no Rust/MSVC. |
| P2 complete | Visible progress for long tasks | Persisted UpdatePlan steps, checks and completion evidence; progress panel, interrupted status and continuation. More agent orchestration remains outside this baseline. |

P0 validation: 82 Rust unit tests plus incremental UTF-8 stream tests and loopback integration checks for early output, session conflicts, cancellation, provider errors, descendant termination, and tool timeout recovery. Unix process cleanup is implemented but has not been exercised on this Windows host. P1/P2 add `scripts/check-features.py` and native Windows input checks; screenshot-to-model payloads are checked with a mock provider. Do not replace the Rust runtime merely to imitate another product's UI.

### Computer control (2026-09-10)

Delivery validation: 112 Rust tests passed. Portable release binaries passed feature and startup checks; chat cancellation preserves attached context, including in non-Git workspaces. Native tests cover owned Windows test windows, occluded WGC capture, stale-state rejection, focus switching, and paired mouse press/release for drag and middle click. Fresh-machine installation and real external vision-model behavior still need broader field testing.

- [x] Local Windows snapshot/accessibility, screenshot, focus, click, drag, move, scroll, Unicode typing and keyboard actions through the built-in Computer tool.
- [x] Default-on Settings > Permissions switch, enforced at tool discovery and execution; disabling cancels active turns.
- [x] Latest current-turn screenshot is passed as an image to the provider. Vision support depends on the selected model.
- [x] No dependency on Zhipu's computer-control service; built-in Windows APIs and PowerShell handle input locally.
- [x] Standard newline-delimited MCP stdio initialization and notifications are supported for independent MCP integrations.
- [x] Codex Windows interface comparison: explicit window identity, indexed controls, direct values and secondary actions, fresh-state/focus/occlusion checks, complete key chords, two-axis scroll and per-action observation.
- [x] Window Graphics Capture backend with compatibility fallback and in-chat screenshot previews. Exact behavior and remaining differences are tracked in `docs/computer-control.md`.

## Next optimization round: Codex / Z Code / Hermes (2026-09-10)

This is a new review round, not a reversal of the delivered P0/P1/P2 baseline.
The six requested optimization areas are implemented as a local baseline. Validation
and operating limits are documented in `docs/autonomous-workflows.md`. This is not
a same-model comparative benchmark or a claim of complete product equivalence.

Delivery verified on 2026-09-11: 120 Rust tests; 17 isolated end-to-end cases;
chat/P1/P2 regressions; native Windows input, modal and occluded-capture checks;
browser UI checks and portable ZIP hash verification. Package binaries passed
the same loopback suites. Real-model and physical mixed-DPI limits remain below.

### R0: correctness and recovery

- [x] R0-1 Builtin file writes enforce workspace roots, traversal/ADS checks and canonical existing-parent containment; explicit full-access remains available.
- [x] R0-2 Standard Chat Completions cached prompt usage is normalized into disjoint input/cache buckets; missing usage/prices remain unknown.
- [x] R0-3 Configurable time/token/iteration limits and repeated action/result detection preserve interrupted progress; token checks run before tools.
- [x] R0-4 Durable incremental checkpoints, acknowledged tool intents/results and restart recovery mark unresolved actions unknown without replaying them.

### R1: task completion and interaction

- [x] R1-1 Explicit goal mode, separate evidence review, bounded continuation and pause/resume; recovered interrupted goals pause for inspection.
- [x] R1-2 Owned Chromium tabs, accessibility references, form input, screenshots, waits, responsive sizes and console/network diagnostics; local iframe fixture tested.
- [x] R1-3 Connection/tools/vision/stream diagnostics, explicit reasoning effort, separate background model name and classified bounded retries. Native protocols beyond Chat Completions are not implemented.
- [x] R1-4 Stable-ID steering and message queue, safe injection boundaries and single-writer session leases.
- [x] R1-5 File/hunk stage, unstage and text revert; snapshots, version checks and ambiguous-edit rejection. Binary/new/deleted/renamed hunk operations return an explicit unsupported result.
- [x] R1-6 Measured cold-start cost, per-worker persistent Computer helper, elapsed timing, process ownership and explicit takeover. Native window/modal checks run separately; physical mixed-DPI multi-display coverage remains below.

### R2: experience and unattended work

- [x] R2-1 Incremental session text index, Chinese substring search, archive/source locations, deletion propagation, memory provenance and reviewable fact conflicts.
- [x] R2-2 Evidence-linked versioned skill candidates with previous/new content review, adopt/reject/disable and opt-in automatic adoption.
- [x] R2-3 Interval/daily IANA-timezone jobs, DST/missed-run policies, atomic claims, existing execution budgets, cancellation and local results inbox; interrupted outcomes pause.
- [x] R2-4 Atomic handoff claims and managed Git worktree creation/listing. Isolation starts from committed HEAD; dependency installation and merges remain explicit workspace operations.

### Remaining validation / extensions

- [ ] Run physical multi-display and mixed-DPI desktop checks on suitable hardware; the current fixture covers owned windows, focus, modal identity, input and occluded capture.
- [ ] Benchmark real external models on the fixed task suite, reporting success and interventions, rather than extrapolating mock-provider results.
- [ ] Add optional OS sandboxing, scoped third-party subprocess credentials, native provider protocols, cross-process iframe support and persistent authenticated browser sessions if needed.
- [ ] Extend scheduling beyond an open local host and the local inbox when a delivery channel is explicitly configured.

Cross-cutting acceptance: maintain a fixed end-to-end task suite recording model,
budget, completion evidence, interventions, duplicated actions, latency and token
usage (foreground and background separately). Unit-test counts alone are not a
real-model task-success benchmark. Before expanding third-party integrations,
add scoped subprocess environments and credential redaction; ProcessTree is not
an OS sandbox.

References: [Codex worktrees](https://learn.chatgpt.com/docs/environments/git-worktrees),
[review](https://learn.chatgpt.com/docs/code-review),
[Windows computer use](https://learn.chatgpt.com/docs/computer-use),
[Z Code goals](https://zcode.z.ai/cn/docs/goal),
[browser](https://zcode.z.ai/cn/docs/browser-use),
[memory](https://zcode.z.ai/cn/docs/memory),
[Hermes memory](https://hermes-agent.nousresearch.com/docs/user-guide/features/memory),
[skills](https://hermes-agent.nousresearch.com/docs/user-guide/features/skills),
[cron](https://hermes-agent.nousresearch.com/docs/user-guide/features/cron),
[Chat Completions usage schema](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create).

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
- Done: remove synchronous provider-based memory selection; use bounded lexical selection. A model-based side-query selector remains optional future work.
- Done: throttle repeated recalled notes and cap the number and characters loaded per turn.
- Done: optional automatic durable-memory extraction runs separately after successful turns.
- Next: keep tightening team-memory sync around richer auth, conflict handling, and large-payload edge cases

### Packaging

- Keep the double-click Windows launcher working from any repo location
- Make desktop/web startup more robust on fresh machines
- Reduce first-run friction in provider setup, skills, MCP, and permissions

## Open items

- Decide whether to add `Start-OpenClaw-Web.bat` and `Stop-OpenClaw.bat`
- Extend portable packaging with installer/update support if needed
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
