# Phase 2 Report

## Goal

The second rebuild phase pushes OpenCoWork beyond the initial runtime skeleton toward a system-level agent harness. The target is not feature parity by copy-paste. The target is architectural parity where the important runtime qualities are preserved and improved:

- explicit session state
- prompt layering and context budgeting
- one runtime loop for model, tools, and permissions
- portable extension surfaces for plugins, skills, and MCP
- deliberate multi-agent division and scheduling

## What this phase adds

- a documented rebuild backlog in `TODOLIST.md`
- prompt orchestration and context selection modules
- persistent local session storage
- an agent scheduling crate for role-based delegation planning
- a skill discovery crate
- an MCP naming and registry crate
- updated CLI wiring so the new architecture is visible from the executable surface

## Design decisions

- OpenCoWork keeps a Rust-first core.
- Prompt assembly is separated from the turn loop so context optimization can evolve independently.
- Skills, plugins, and MCP are treated as different extension surfaces:
  - skills shape instructions and workflows
  - plugins contribute local executable tools and hooks
  - MCP contributes externally hosted or proxied tool surfaces
- Multi-agent routing is modeled as planning and assignment first, not immediate process spawning. This keeps the scheduler testable.

## Current gaps after this phase

- no production-grade provider adapter yet
- no real streaming transport for model output
- no live MCP transport client yet
- no persistent multi-agent runtime execution engine yet
- prompt budgeting still uses heuristic token estimates rather than provider-native counts

## Why this is still the correct direction

The existing codebase is now organized around the right control planes. That matters more than shipping a thin demo with the wrong architecture. From this point, the next iterations can deepen behavior without needing another repository reset.
