# Phase 8 Report

## Scope

This phase tightened OpenCoWork's skill and prompt orchestration so it tracks the reconstructed Claude Code repositories more closely before any web or EXE shell work begins.

## Implemented

- Expanded skill discovery roots in [crates/opencowork-cli/src/main.rs](d:\新建程序项目\opencowork\crates\opencowork-cli\src\main.rs) to include `.claude`, `.opencowork`, and `.codex` `skills/` and legacy `commands/` directories.
- Reworked legacy command loading in [crates/skills/src/lib.rs](d:\新建程序项目\opencowork\crates\skills\src\lib.rs) so nested markdown commands are discovered recursively and `SKILL.md` takes precedence over sibling markdown files in the same command directory.
- Added namespaced legacy command naming in [crates/skills/src/lib.rs](d:\新建程序项目\opencowork\crates\skills\src\lib.rs), producing command-style names such as `ops:deploy` instead of flat file stems.
- Expanded skill frontmatter parsing in [crates/skills/src/lib.rs](d:\新建程序项目\opencowork\crates\skills\src\lib.rs) and [crates/tools/src/lib.rs](d:\新建程序项目\opencowork\crates\tools\src\lib.rs) to preserve `when_to_use`, `argument_hint`, `allowed_tools`, `context`, `version`, `agent`, `model`, and `effort`.
- Upgraded prompt-side skill reinjection in [crates/runtime/src/prompt.rs](d:\新建程序项目\opencowork\crates\runtime\src\prompt.rs) so active skill instructions now carry more of the loaded frontmatter guidance into later turns.
- Cleaned the CLI `skills` view in [crates/opencowork-cli/src/main.rs](d:\新建程序项目\opencowork\crates\opencowork-cli\src\main.rs) so debugging skill origin and execution context is easier while the future UI shell does not exist yet.

## Behavior Changes

- OpenCoWork now sees the same broad skill directory families as the reference repos instead of only its own local `skills/` paths.
- Legacy markdown command trees behave more like the reference repo: nested commands can be discovered, and a directory-level `SKILL.md` wins over ordinary markdown files beside it.
- A skill loaded through the `Skill` tool now preserves more execution guidance for subsequent prompt construction instead of collapsing to raw markdown only.

## Validation

- `cargo fmt`
- `cargo test --target-dir "$env:TEMP\opencowork-target"`
- `cargo clippy --all-targets --all-features --target-dir "$env:TEMP\opencowork-target" -- -D warnings`

## Remaining Gap

- Conditional skill activation based on touched file paths is still missing.
- Hook metadata can now be parsed conceptually from the reference, but OpenCoWork does not yet execute the richer reference-style hook matrix.
- Deferred activation still covers `Skill` only; heavier semantic tools such as LSP remain future parity work.
