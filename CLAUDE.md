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
cargo check
cargo build
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
| `commands/mod.rs` | Tauri IPC command handlers - entry point for all frontend calls |
| `capture/mod.rs` | Screen capture loop with perceptual hash comparison to skip unchanged frames |
| `capture/screen.rs` | Screenshot capture and base64 encoding |
| `model/mod.rs` | Unified AI model interface |
| `model/api.rs` | OpenAI/Claude API client |
| `model/ollama.rs` | Ollama local model client |
| `storage/mod.rs` | Config, SummaryRecord, AggregatedRecord, smart search |

### Key Frontend Files (`src/`)

| File | Purpose |
|------|---------|
| `views/MainView.vue` | Chat interface, capture controls, alert listener |
| `views/SettingsView.vue` | Model/capture/storage configuration |
| `views/HistoryView.vue` | Timeline of recorded activities |
| `stores/capture.ts` | Capture state management |

### Important Patterns

**Tauri Commands**: All backend functions exposed to frontend are in `commands/mod.rs` with `#[tauri::command]` attribute.

**Event Emission**: Backend emits `assistant-alert` events when errors detected on screen. Frontend listens via `@tauri-apps/api/event`.

**Frame Skipping**: `capture/mod.rs` uses 8x8 perceptual hash to compare frames. Similarity above threshold (default 0.95) skips AI analysis to save tokens.

**Two-Layer Storage**:
- Raw `SummaryRecord` per capture
- `AggregatedRecord` every 300 records (~5 min)
- Smart search uses aggregated data for longer time ranges

### Data Storage Location
```
{LocalAppData}/screen-assistant/data/
├── config.json
└── summaries/YYYY-MM-DD.json
```

## Configuration Structure

Key config fields in `storage/mod.rs`:
- `model.provider`: "api" | "ollama"
- `capture.interval_ms`: Screenshot interval (default 1000ms)
- `capture.skip_unchanged`: Enable frame comparison (default true)
- `capture.change_threshold`: Similarity threshold 0.0-1.0 (default 0.95)
- `storage.max_context_chars`: Max chars for AI context (default 10000)
