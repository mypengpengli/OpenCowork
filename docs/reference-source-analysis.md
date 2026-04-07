# Reference Source Analysis

## Goal

This note decides which external source trees should drive OpenCoWork's core runtime and tool design.

## Sources checked

### 1. Local mirror

- Local path: [`.tmp/claw-code-main`](d:\新建程序项目\opencowork\.tmp\claw-code-main)
- This repository is a clean-room rewrite project. The current active implementation is split between a Python tree in `src/` and a Rust workspace in `rust/`.
- Evidence:
  - [README.md](d:\新建程序项目\opencowork\.tmp\claw-code-main\README.md)
  - [PARITY.md](d:\新建程序项目\opencowork\.tmp\claw-code-main\PARITY.md)

### 2. `ChinaSiro/claude-code-sourcemap`

- URL: https://github.com/ChinaSiro/claude-code-sourcemap
- This is a source-map based restoration of Claude Code assets. It is valuable as a lookup source for original names, packed layouts, and recovered JavaScript/TypeScript behavior.
- It is not the best primary implementation target for OpenCoWork because it is closer to recovered source than a clean architectural baseline.

### 3. `claude-code-best/claude-code`

- URL: https://github.com/claude-code-best/claude-code
- This is the most useful primary behavioral reference for the original tool and command surface because it is organized as a reconstructed runnable repository instead of only a recovered bundle.
- It is the best source for checking which tools, command paths, hook surfaces, and orchestration layers existed in the original system.

## Conclusion

These three sources are not the same thing:

- `claude-code-best/claude-code` is the primary source for feature surface and behavior parity.
- `ChinaSiro/claude-code-sourcemap` is a secondary lookup source when behavior exists but the reconstructed repo is missing detail.
- `.tmp/claw-code-main` is a secondary architectural reference for clean-room Rust/Python implementation patterns, but it is not a parity source for the original TypeScript CLI.

## What OpenCoWork should follow

For core runtime and tools, the order of precedence should be:

1. `claude-code-best/claude-code`
2. `ChinaSiro/claude-code-sourcemap`
3. local [`.tmp/claw-code-main`](d:\新建程序项目\opencowork\.tmp\claw-code-main)

That means:

- tool inventory and orchestration should be checked first against the restored Claude Code repos
- hook behavior, command surfaces, and skill loading rules should be matched against the restored Claude Code repos
- Rust implementation style, modularization, and some clean-room simplifications can continue to borrow from the local `.tmp` mirror

## Impact on OpenCoWork

OpenCoWork should keep its own naming and product shell, but the following subsystems should now be treated as parity-driven work rather than local invention:

- tool registry and deferred tool activation
- prompt assembly and context layering
- compaction and summary rollover behavior
- skill injection and command discovery
- hook execution boundaries
- multi-agent handoff plumbing where the restored repos provide evidence
