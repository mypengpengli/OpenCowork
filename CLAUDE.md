# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development Commands

```bash
# Start development (frontend + backend together)
npm run tauri dev

# Build for production
npm run tauri build

# Frontend only development (Vite dev server on port 1420)
npm run dev

# Type checking
vue-tsc --noEmit

# Rust-only commands (from src-tauri directory)
cd src-tauri
cargo check
cargo build
cargo clippy          # Lint Rust code
cargo test            # Run Rust tests
```

## Architecture Overview

Screen Assistant is a Tauri 2 desktop application that monitors screen activity using AI vision models and provides an intelligent assistant for querying recent activities.

### Tech Stack
- **Frontend**: Vue 3 + TypeScript + Naive UI + Pinia
- **Backend**: Rust (Tauri 2) with tokio async runtime
- **AI**: OpenAI/Claude API (cloud) or Ollama (local) for vision analysis

### Data Flow
```
Vue Frontend (IPC) → Tauri Commands → Rust Backend
                                         ├── CaptureManager (screenshot loop)
                                         ├── ModelManager (AI API calls)
                                         └── StorageManager (JSON persistence)
```

### Key Backend Modules (`src-tauri/src/`)

| Module | Purpose |
|--------|---------|
| `lib.rs` | Tauri app setup, command registration |
| `commands/mod.rs` | Tauri IPC command handlers - entry point for all frontend calls |
| `capture/mod.rs` | Screen capture loop with perceptual hash comparison to skip unchanged frames |
| `capture/screen.rs` | Screenshot capture and base64 encoding |
| `model/mod.rs` | ModelManager - unified AI model interface with Tool Use support |
| `model/api.rs` | OpenAI/Claude API client with function calling |
| `model/ollama.rs` | Ollama local model client |
| `storage/mod.rs` | Config, SummaryRecord, AggregatedRecord, smart search |
| `skills/mod.rs` | SkillManager - skill discovery, loading, creation |
| `skills/parser.rs` | SKILL.md parser (YAML frontmatter + Markdown) |

### Key Frontend Files (`src/`)

| File | Purpose |
|------|---------|
| `views/MainView.vue` | Chat interface, capture controls, `/skill-name` syntax detection |
| `views/SettingsView.vue` | Profile management, model/capture/storage config, Skills management |
| `views/HistoryView.vue` | Timeline of recorded activities |
| `stores/capture.ts` | Capture state management with auto-restart |
| `stores/chat.ts` | Chat messages state |
| `stores/skills.ts` | Skills state management (list, create, delete) |

### Important Patterns

**Tauri Commands**: All backend functions exposed to frontend are in `commands/mod.rs` with `#[tauri::command]` attribute. Commands are registered in `lib.rs`.

**Event Emission**: Backend emits `assistant-alert` events when errors detected on screen. Frontend listens via `@tauri-apps/api/event`.

**Frame Skipping**: `capture/mod.rs` uses 8x8 perceptual hash to compare frames. Similarity above threshold (default 0.95) skips AI analysis to save tokens.

**Two-Layer Storage**:
- Raw `SummaryRecord` per capture
- `AggregatedRecord` every 300 records (~5 min)
- Smart search uses aggregated data for longer time ranges

**Natural Language Query Parsing**: `storage/mod.rs` contains `smart_search` that parses time expressions like "刚才", "最近N分钟", "今天", "昨天" and extracts keywords.

### Data Storage Location
```
Windows: %LOCALAPPDATA%\screen-assistant\data\
macOS:   ~/Library/Application Support/screen-assistant/data/
Linux:   ~/.local/share/screen-assistant/data/

Structure:
├── config.json              # Current configuration
├── profiles/                # Named configuration profiles
├── summaries/YYYY-MM-DD.json  # Daily activity records
└── logs/                    # API exchange logs
```

Note: Screenshots are NOT saved to disk by default. They are converted to base64, sent to AI for analysis, and only the text summary is persisted.

## Configuration Structure

Key config fields in `storage/mod.rs`:
- `model.provider`: "api" | "ollama"
- `model.api.type`: "openai" | "claude" | "custom"
- `capture.interval_ms`: Screenshot interval (default 1000ms)
- `capture.skip_unchanged`: Enable frame comparison (default true)
- `capture.change_threshold`: Similarity threshold 0.0-1.0 (default 0.95)
- `capture.recent_summary_limit`: Max recent summaries for context
- `storage.max_context_chars`: Max chars for AI context (default 10000)
- `storage.retention_days`: How long to keep history (default 7)

## Skills System

Skills are reusable AI capabilities that can be invoked manually (`/skill-name`) or automatically via Tool Use.

### Skills Directory Structure
```
data/skills/
├── calculator/
│   └── SKILL.md
├── export/
│   └── SKILL.md
└── custom-skill/
    └── SKILL.md
```

### SKILL.md Format
```markdown
---
name: skill-name
description: Description used by AI to decide when to invoke this skill
metadata:
  author: screen-assistant
  version: "1.0"
---

# Skill Title

## Instructions
Markdown content with skill instructions...
```

### Skill Invocation Flow
1. **Manual**: User types `/skill-name args` in chat
2. **Auto (Tool Use)**: AI decides to call `invoke_skill` or `manage_skill` tool based on user request (API mode only)

### Key Skill Commands (Tauri)
- `list_skills`: Get all available skill metadata
- `get_skill`: Load full skill with instructions
- `invoke_skill`: Execute a skill with args
- `create_skill`: Create new skill
- `delete_skill`: Remove a skill

### Tool Use Implementation
- `model/api.rs`: `create_skill_tools()` creates two tools:
  - `invoke_skill`: Call an existing skill
  - `manage_skill`: Create/update/delete skills (action: "create" | "update" | "delete")
- `commands/mod.rs`: `chat_with_assistant()` handles tool calls loop
- Only works with API providers (OpenAI/Claude), Ollama falls back to text hints
