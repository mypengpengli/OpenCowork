# OpenCowork - Your AI Work Companion

OpenCowork keeps things lightweight and practical. It focuses on two core capabilities: screen monitoring and the Skills system. Skills are infinitely extensible, and OpenCowork includes built-in creation, management, update, and deletion of Skills. You can ask the model to generate a Skill for any workflow, or import an existing Skill. If a Skill has flaws, you can ask the model to revise it or edit `SKILL.md` yourself.

[![Version](https://img.shields.io/badge/version-0.2.5-blue.svg)](https://github.com/mypengpengli/OpenCowork)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Stars](https://img.shields.io/github/stars/mypengpengli/OpenCowork?style=social)](https://github.com/mypengpengli/OpenCowork)

**[English](#english) | [中文](#中文)**

---

<a name="english"></a>

> **Never miss an error message again.**

Have you ever experienced this?

- During deployment, a red error flashes by before you can read it
- While debugging, the error message is too long and gets overwritten
- You want to ask AI for help but can not remember what the error was
- Constantly switching between coding and searching, breaking your flow

**OpenCowork was built to solve these pain points.**

## What Can It Do?

- **Always-on screen monitoring**: capture errors, warnings, and key on-screen events
- **Natural recall**: ask questions like "what error just happened" or "what failed in the last 10 minutes"
- **Skills system**: reusable AI workflows you can create, update, and invoke with `/skill-name`
- **AI-managed Skills**: describe what you want, let AI generate or refine the Skill
- **Background progress**: see real-time plans, milestones, and tool steps while AI works
- **Global prompts**: inject personal or team info into every conversation
- **Smart frame skipping**: skip analysis when the screen does not change
- **Privacy-first**: data is stored locally, with retention controls

## Recent Updates

- Background progress panel with plan and milestone updates
- Tool step cards shown in assistant replies
- Rich Markdown rendering (GFM tables, code blocks, lists) with safe sanitization
- Long output folding with expand/collapse
- Paste screenshots directly into the chat input (Ctrl+V)
- Workspace picker for tool allowed directories (first line is default)

## Use Cases

| Scenario | Pain Point | OpenCowork's Solution |
|----------|------------|-----------------------|
| Deployment | Logs scroll too fast | Automatically capture and save errors |
| Debugging | Error too long to copy | Full recording, query anytime |
| Learning | Unsure why it failed | AI summarizes and suggests fixes |
| Remote collaboration | Hard to describe issues | Export records to reproduce problems |
| Work review | Forgot what you did | Natural language queries by time period |

## Technical Highlights

- **Tauri 2 + Rust**: native performance, minimal resource usage
- **Vue 3 + TypeScript**: modern frontend
- **Dual model support**: cloud API (OpenAI/Claude) or local Ollama
- **Two-layer storage**: raw records + smart aggregation
- **Skills hot reload**: edits to `SKILL.md` are picked up automatically
- **Tool use support**: AI can create, modify, and delete Skills

## Quick Start

### Requirements

- Node.js 18+
- Rust 1.70+
- Optional: Ollama (for local models)

### Installation

```bash
git clone https://github.com/mypengpengli/OpenCowork.git
cd OpenCowork
npm install
npm run tauri dev
```

### Configure AI Model

#### Cloud API (Recommended)

1. Settings -> Model Source -> `API (Cloud)`
2. Select API Type: `OpenAI` / `Claude` / `Custom`
3. Enter API URL and key
4. Recommended models: `gpt-4o` or `claude-3-opus-20240229`

#### Local Ollama

```bash
ollama pull llava
```

Then select `Ollama (Local)` in settings, URL: `http://localhost:11434`

## Data Storage

```
Windows: %LOCALAPPDATA%\opencowork\datamacOS:   ~/Library/Application Support/opencowork/data/
Linux:   ~/.local/share/opencowork/data/
```

Skills live under `<data>/skills` and edits are picked up automatically.

**Privacy Guarantee**:
- All data stored locally only
- Screenshots are stored locally and governed by retention settings
- Images are sent to AI providers only during API calls

## Build

```bash
# Development
npm run tauri dev

# Production
npm run tauri build
```

## License

MIT

---

<a name="中文"></a>

# OpenCowork - 你的 AI 工作伙伴

OpenCowork 保持轻量和实用。它专注于两个核心能力：**屏幕监控** 和 **Skills 系统**。Skills 可以无限扩展，OpenCowork 内置了创建、管理、更新和删除 Skills 的功能。你可以让模型为任何工作流生成 Skill，或导入现有 Skill。如果 Skill 有缺陷，你可以让模型修改它或自己编辑 `SKILL.md`。

> **再也不会错过任何错误信息**

你是否遇到过这些情况？

- 部署时红色错误一闪而过来不及看清
- 调试时错误信息太长被覆盖
- 想问 AI 帮忙却记不清错误内容
- 不断在编码和搜索间切换打断思路

**OpenCowork 就是为解决这些痛点而生**

## 核心功能

- **常驻监控**：自动捕获错误、警告等关键信息
- **自然回忆**：问"刚才什么错误"或"最近 10 分钟发生了什么"
- **Skills 系统**：可复用的 AI 工作流，用 `/skill-name` 快速调用
- **AI 管理 Skills**：描述需求，AI 自动生成或优化 Skill
- **后台进度面板**：实时显示计划、里程碑和工具步骤
- **全局提示词**：注入个人/团队信息到每次对话
- **智能跳帧**：屏幕无变化时跳过分析节省开销
- **隐私优先**：数据本地存储，可控保留时长

## 最近更新

- 后台进度面板，显示计划和里程碑更新
- 助手回复中显示工具步骤卡片
- Markdown 富文本渲染（表格、代码块、列表）和安全清理
- 长输出折叠，支持展开/收起
- 粘贴截图到聊天输入框（Ctrl+V）
- 工作区选择器，用于工具允许的目录（首行为默认）

## 使用场景

| 场景 | 痛点 | OpenCowork 的解决方案 |
|------|------|---------------------|
| 部署 | 日志滚动太快看不清 | 自动捕获并保存错误 |
| 调试 | 错误信息太长无法复制 | 完整记录随时查询 |
| 学习 | 不确定为什么失败 | AI 总结并给出修复建议 |
| 远程协作 | 难以描述问题 | 导出记录重现问题 |
| 工作回顾 | 忘记做了什么 | 按时间段自然语言查询 |

## 技术亮点

- **Tauri 2 + Rust**：原生性能，最小资源占用
- **Vue 3 + TypeScript**：现代化前端
- **双模型支持**：云端 API（OpenAI/Claude）或本地 Ollama
- **双层存储**：原始记录 + 智能聚合
- **Skills 热重载**：编辑 `SKILL.md` 自动生效
- **Tool Use 支持**：AI 可创建/修改/删除 Skills

## 快速开始

### 环境要求

- Node.js 18+
- Rust 1.70+
- 可选：Ollama（用于本地模型）

### 安装

```bash
git clone https://github.com/mypengpengli/OpenCowork.git
cd OpenCowork
npm install
npm run tauri dev
```

### 配置 AI 模型

#### 云端 API（推荐）

1. 设置 -> 模型来源 -> `API (云端)`
2. 选择 API 类型：`OpenAI` / `Claude` / `自定义`
3. 输入 API 地址和密钥
4. 推荐模型：`gpt-4o` 或 `claude-3-opus-20240229`

#### 本地 Ollama

```bash
ollama pull llava
```

然后在设置中选择 `Ollama (本地)`，地址：`http://localhost:11434`

## 数据存储

```
Windows: %LOCALAPPDATA%\opencowork\datamacOS:   ~/Library/Application Support/opencowork/data/
Linux:   ~/.local/share/opencowork/data/
```

Skills 位于 `<data>/skills`，编辑后自动生效。

**隐私保证**：
- 所有数据仅本地存储
- 截图本地存储，受保留设置管理
- 仅在 API 调用时发送图片给 AI 提供商

## 构建

```bash
# 开发
npm run tauri dev

# 生产
npm run tauri build
```

## License

MIT
