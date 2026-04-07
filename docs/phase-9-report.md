# Phase 9 Report

## Scope

This phase focused on the next layer of reference-driven mechanics: conditional skills that activate from file activity, and a broader runtime hook event model that is not limited to plain pre/post tool hooks.

## Implemented

- Added skill `paths` frontmatter parsing and conditional matching in [crates/skills/src/lib.rs](d:\新建程序项目\opencowork\crates\skills\src\lib.rs).
- Added conditional skill prompt injection in [crates/opencowork-cli/src/main.rs](d:\新建程序项目\opencowork\crates\opencowork-cli\src\main.rs) by scanning previously touched `read_file`, `write_file`, and `edit_file` paths from session history.
- Expanded `Skill` tool payloads and prompt-side skill reinjection metadata with `paths` in [crates/tools/src/lib.rs](d:\新建程序项目\opencowork\crates\tools\src\lib.rs) and [crates/runtime/src/prompt.rs](d:\新建程序项目\opencowork\crates\runtime\src\prompt.rs).
- Reworked hook config parsing in [crates/runtime/src/config.rs](d:\新建程序项目\opencowork\crates\runtime\src\config.rs) to support a broader event set plus richer command-hook objects with `matcher` / `if` and `once`.
- Rebuilt the hook runner in [crates/runtime/src/hooks.rs](d:\新建程序项目\opencowork\crates\runtime\src\hooks.rs) around event-based dispatch instead of only two fixed command lists.
- Wired runtime hook execution into more lifecycle points in [crates/runtime/src/conversation.rs](d:\新建程序项目\opencowork\crates\runtime\src\conversation.rs), including session start, user prompt submit, permission denied, tool failure, and session end.
- Fixed CLI hook assembly in [crates/opencowork-cli/src/main.rs](d:\新建程序项目\opencowork\crates\opencowork-cli\src\main.rs) so config-defined hooks and plugin hooks are merged instead of silently dropping config hooks.

## Behavior Changes

- Skills can now stay dormant until relevant files have actually been touched in prior session work.
- Conditional skills are injected automatically into the next prepared prompt rather than requiring explicit `Skill` tool usage first.
- Hooks are no longer limited to just `PreToolUse` and `PostToolUse`; the runtime can now react to broader session/control events with command hooks.
- Hook config from `settings.json` finally participates in live execution together with plugin hooks.

## Validation

- `cargo fmt`
- `cargo test --target-dir "$env:TEMP\opencowork-target"`
- `cargo clippy --all-targets --all-features --target-dir "$env:TEMP\opencowork-target" -- -D warnings`

## Remaining Gap

- Conditional skill activation is still next-turn prompt preparation, not same-turn hot activation while file tools are firing.
- Hook execution still supports command hooks only; prompt, HTTP, and agent hook transports from the reference repos are not implemented yet.
- Hook matcher support is intentionally simpler than the reference tree-based matcher model.
