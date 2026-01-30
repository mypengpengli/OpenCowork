# OpenCowork - Your AI Work Companion

OpenCowork is designed to stay simple and lightweight. It focuses on two core capabilities: screen monitoring and the Skills system. Because Skills are infinitely extensible, OpenCowork includes built-in creation, management, update, and deletion of Skills. You can ask the model to generate a Skill for any workflow (for example, a file-organization Skill), or import an existing Skill. If a Skill has flaws, you can have the model revise it or edit `SKILL.md` yourself.

[![Version](https://img.shields.io/badge/version-0.2.5-blue.svg)](https://github.com/mypengpengli/OpenCowork)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Stars](https://img.shields.io/github/stars/mypengpengli/OpenCowork?style=social)](https://github.com/mypengpengli/OpenCowork)

**[English](#english) | [中文](#中文)**

---

<a name="english"></a>

> **Never miss an error message again.**

Have you ever experienced this?

- During deployment, a red error flashes by in the terminal before you can read it
- While debugging, the error message is too long and gets overwritten before you can copy it
- You want to ask AI for help but can't remember what the error was
- Constantly switching between coding and searching, breaking your flow

**OpenCowork was built to solve these pain points.**

## What Can It Do?

## Recent updates

- Rich chat rendering: Markdown (GFM tables, code blocks, lists) with safe sanitization
- Tool steps cards: backend progress steps are shown as compact cards in assistant replies
- Long output folding: lengthy messages auto-collapse with expand/collapse

### 🎯 Automatically Capture Every Error

From the moment you click "Start Monitoring", OpenCowork works like a tireless assistant, continuously watching your screen. Whether it's compilation errors, runtime exceptions, or console warnings — **every detail is recorded**.

When an error is detected, AI proactively notifies you:
- What error occurred
- Possible causes
- Suggested solutions

**No more frantically taking screenshots or copy-pasting.**

### 💬 Instant Recall with Natural Conversation

Forgot what that error was? Just ask:

- *"What was that error just now?"*
- *"What errors did I encounter in the last 10 minutes?"*
- *"How many compilation failures did I have this afternoon?"*

OpenCowork understands natural language and supports multi-turn conversations, like chatting with a colleague who knows your entire work history.

### ?? Intent Recognition & Proactive Assistance

OpenCowork infers your current intent and scene from on-screen context (coding, browsing, form-filling, etc.). It uses that signal to surface timely hints and suggest the most relevant Skills.


### 🔧 Skills System — Infinitely Extensible

This is OpenCowork's most powerful feature.

**Skills are reusable AI capability modules**. You can:

- Type `/export` to export activity records as a report
- Type `/analyze` for AI to deeply analyze your work patterns
- Create custom Skills to have AI perform specific tasks

Even more powerful: **AI can automatically invoke Skills**. When you say "summarize my work today", AI automatically determines which Skill to use.

### 🚀 2.0 New Feature: AI-Managed Skills

**This is a revolutionary update.**

In version 2.0, you no longer need to manually write SKILL.md files. Just tell AI in natural language:

- *"Create a code review skill for me"* → AI automatically generates the complete Skill
- *"Modify the export skill to support Markdown format"* → AI automatically updates the Skill
- *"Delete the test skill"* → AI automatically cleans up

**AI becomes your skill factory.** Just describe your needs, and AI will:
1. Understand your intent
2. Design the skill structure
3. Write detailed instructions
4. Automatically save to the system

This means:
- **Zero barrier to creation**: No need to understand SKILL.md format or write code
- **Rapid iteration**: Skill not working well? One sentence to improve it
- **Unlimited possibilities**: Any workflow you can imagine can become a Skill

### 💡 Real-World Examples

Here are some practical examples of creating Skills with just one sentence:

| Your Request | What AI Creates |
|--------------|-----------------|
| *"Create a skill to convert video formats"* | A video-convert skill that guides you through using FFmpeg to convert between MP4, AVI, MKV, etc. |
| *"I need a skill to compress images in batch"* | An image-compress skill with instructions for batch processing using ImageMagick or similar tools |
| *"Help me create a skill for generating git commit messages"* | A commit-msg skill that analyzes your changes and suggests conventional commit messages |
| *"Create a skill to explain code errors"* | An error-explain skill that provides detailed explanations and solutions for common programming errors |
| *"I want a skill to summarize meeting notes"* | A meeting-summary skill that extracts action items, decisions, and key points from your notes |

**Just say what you need, and AI handles the rest!**

**For enterprise users**: Create team-specific Skills to standardize workflows and boost collaboration.

**For individual developers**: Encapsulate common operations into Skills, completing complex tasks with a single sentence.

### 📝 Global Prompts — Your Personal Info, Always Ready

Tired of repeatedly telling AI your company name, department, or manager's name when filling out forms?

**Global Prompts** let you save frequently used personal information that automatically gets injected into every AI conversation:

- Save your company info: *"I work at Acme Corp, Engineering Department"*
- Save your team info: *"My manager is John Smith, my director is Jane Doe"*
- Save common formats: *"Reports should use the company template with header..."*

**How it works:**
1. Go to Settings → Global Prompts tab
2. Create prompts with names like "Personal Info" or "Company Info"
3. Toggle them on/off as needed
4. AI automatically uses this info when relevant (e.g., filling forms, writing emails)

**No more copy-pasting the same info over and over!**

### 🧠 Smart Frame Skipping, Save Money

Worried about token consumption? OpenCowork uses perceptual hashing to compare frames. **When the screen hasn't changed, analysis is automatically skipped**, significantly reducing API costs while ensuring no important information is missed.

## Use Cases

| Scenario | Pain Point | OpenCowork's Solution |
|----------|------------|----------------------------|
| **Deployment** | Logs scroll too fast, errors flash by | Automatically capture and save all errors |
| **Debugging** | Error too long, can't copy in time | Complete recording, query anytime |
| **Learning** | Don't know what went wrong | AI proactively analyzes and suggests |
| **Remote Collaboration** | Hard to describe the problem | Export records to precisely reproduce issues |
| **Work Review** | Forgot what you did today | Natural language query for any time period |

## Technical Highlights

- **Tauri 2 + Rust**: Native performance, minimal resource usage
- **Vue 3 + TypeScript**: Modern frontend, smooth experience
- **Intent Recognition**: Detects user intent/scene to drive proactive tips and skill suggestions
- **Dual Model Support**: Cloud API (OpenAI/Claude) or local Ollama
- **Two-Layer Storage**: Raw records + smart aggregation, balancing detail and efficiency
- **Privacy First**: All data stored locally; screenshots are saved on disk and governed by retention settings
- **Skills Hot Reload**: Edit `SKILL.md` and the in-app list updates automatically
- **Tool Use Support**: AI can autonomously create, modify, and delete skills
- **Global Prompts**: Save personal info once, auto-inject into every conversation

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

1. Settings → Model Source → `API (Cloud)`
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
Windows: %LOCALAPPDATA%\opencowork\data\
macOS:   ~/Library/Application Support/opencowork/data/
Linux:   ~/.local/share/opencowork/data/
```

Skills live under `<data>/skills` and edits are picked up automatically.

**Privacy Guarantee**:
- All data stored locally only
- Screenshots are stored locally for analysis and can be purged by retention settings
- Images are sent to AI providers during API calls

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

# OpenCowork - 你的 AI 工作伴侣

OpenCowork 以简单轻便为主，只提供屏幕监控和 Skills 两大核心能力。由于 Skills 可以无限扩展，工具内置了创建、管理、修改、删除 Skills 的功能。你可以让大模型按任何工作需求生成 Skill（例如“整理文件”的 Skill），也可以直接导入已有 Skill；如果已有 Skill 有瑕疵，可以让大模型修改，或者你自己编辑 `SKILL.md`。

> **再也不用担心错过任何一个报错信息了。**

你是否有过这样的经历？

- 部署项目时，终端里一闪而过的红色报错，等你反应过来已经被新日志刷走了
- 调试代码时，错误信息太长，还没来得及复制就被覆盖了
- 想问 AI 帮忙解决问题，却记不清刚才的报错内容是什么
- 一边写代码一边查百度/Google，来回切换窗口打断思路

**OpenCowork 就是为解决这些痛点而生的。**

## 它能做什么？

## 最近更新

- 对话渲染增强：支持 Markdown（表格/代码块/列表）并做安全净化
- 工具步骤卡片：把后台过程步骤以卡片形式附在回复下
- 长内容折叠：长消息自动折叠，可展开/收起

### 🎯 自动捕获每一个错误

从你点击「开始监控」的那一刻起，OpenCowork 就像一个不知疲倦的助手，持续观察你的屏幕。无论是编译错误、运行时异常、还是控制台警告——**每一个细节都会被记录下来**。

当检测到错误时，AI 会主动推送提醒，告诉你：
- 发生了什么错误
- 可能的原因是什么
- 建议如何解决

**你再也不需要手忙脚乱地截图、复制粘贴了。**

### 💬 随时回溯，自然对话

忘记刚才的报错内容？没关系，直接问：

- *"刚才那个报错是什么？"*
- *"最近 10 分钟我遇到了哪些错误？"*
- *"今天下午编译失败了几次？"*

OpenCowork 理解自然语言，支持多轮对话，就像和一个了解你所有操作历史的同事聊天一样。

### 🧭 意图识别与主动帮助

OpenCowork 会基于屏幕内容识别当前意图和场景（如编码、浏览、填表等），并据此主动提示、推荐合适的技能。

### 🔧 Skills 系统 —— 无限扩展的能力

这是 OpenCowork 最强大的特性。

**Skills 是可复用的 AI 能力模块**，你可以：

- 输入 `/export` 将操作记录导出为报告
- 输入 `/analyze` 让 AI 深度分析你的工作模式
- 创建自定义 Skill，让 AI 按照你的需求执行特定任务

更强大的是，**AI 可以自动调用 Skills**。当你说"帮我总结一下今天的工作"，AI 会自动判断需要调用哪个 Skill 来完成任务。

### 🚀 2.0 新特性：AI 自主管理 Skills

**这是一个革命性的更新。**

在 2.0 版本中，你不再需要手动编写 SKILL.md 文件。只需用自然语言告诉 AI：

- *"帮我创建一个代码审查技能"* → AI 自动生成完整的 Skill
- *"修改 export 技能，让它支持 Markdown 格式"* → AI 自动更新 Skill 内容
- *"删除 test 技能"* → AI 自动清理

**AI 成为了你的技能工厂。** 你只需要描述需求，AI 会：
1. 理解你的意图
2. 设计技能结构
3. 编写详细指令
4. 自动保存到系统

这意味着：
- **零门槛创建**：不需要了解 SKILL.md 格式，不需要写代码
- **快速迭代**：发现技能不好用？一句话让 AI 改进
- **无限可能**：任何你能想到的工作流，都可以变成一个 Skill

### 💡 实际应用示例

以下是一些一句话创建 Skill 的实际例子：

| 你的请求 | AI 创建的技能 |
|---------|--------------|
| *"创建一个转换视频格式的技能"* | 一个 video-convert 技能，指导你使用 FFmpeg 在 MP4、AVI、MKV 等格式之间转换 |
| *"我需要一个批量压缩图片的技能"* | 一个 image-compress 技能，包含使用 ImageMagick 等工具批量处理的指令 |
| *"帮我创建一个生成 git 提交信息的技能"* | 一个 commit-msg 技能，分析你的代码变更并建议规范的提交信息 |
| *"创建一个解释代码错误的技能"* | 一个 error-explain 技能，为常见编程错误提供详细解释和解决方案 |
| *"我想要一个总结会议记录的技能"* | 一个 meeting-summary 技能，从你的笔记中提取行动项、决策和要点 |

**只需说出你的需求，AI 帮你搞定一切！**

**对于企业用户**：可以为团队定制专属 Skills，统一工作流程，提升协作效率。

**对于个人开发者**：可以把常用的操作封装成 Skill，一句话完成复杂任务。

### 📝 全局提示词 — 个人信息，随时就绪

是否厌倦了每次填表单时都要告诉 AI 你的公司名称、部门或领导姓名？

**全局提示词**让你保存常用的个人信息，自动注入到每次 AI 对话中：

- 保存公司信息：*"我是XX公司的员工，隶属于技术部"*
- 保存团队信息：*"我的部门经理是张三，分管领导是李四"*
- 保存常用格式：*"报告需要使用公司模板，包含页眉..."*

**使用方法：**
1. 进入设置 → 全局提示词标签页
2. 创建提示词，如"个人信息"或"公司信息"
3. 根据需要开启或关闭
4. AI 会在相关场景自动使用这些信息（如填写表单、撰写邮件）

**再也不用反复复制粘贴同样的信息了！**

### 🧠 智能跳帧，省钱省心

担心 Token 消耗太快？OpenCowork 使用感知哈希算法对比画面，**当屏幕没有变化时自动跳过分析**，在保证不遗漏任何重要信息的同时，大幅降低 API 调用成本。

## 适用场景

| 场景 | 痛点 | OpenCowork 的解决方案 |
|------|------|---------------------------|
| **项目部署** | 日志刷屏，错误一闪而过 | 自动捕获并保存所有错误信息 |
| **代码调试** | 报错太长，来不及复制 | 完整记录，随时回溯查询 |
| **学习编程** | 不知道自己哪里做错了 | AI 主动分析错误原因并给出建议 |
| **远程协作** | 难以描述遇到的问题 | 导出操作记录，精准还原问题现场 |
| **工作复盘** | 忘记今天做了什么 | 自然语言查询任意时间段的操作 |

## 技术亮点

- **Tauri 2 + Rust**：原生性能，极低资源占用
- **Vue 3 + TypeScript**：现代化前端，流畅体验
- **意图识别**：识别用户意图/场景，驱动主动提示与技能推荐
- **双模型支持**：云端 API (OpenAI/Claude) 或本地 Ollama，灵活选择
- **两层存储架构**：原始记录 + 智能聚合，平衡详细度与存储效率
- **隐私优先**：所有数据本地存储；截图会保存在本地并受保留策略控制
- **Skills 热重载**：编辑 `SKILL.md` 后列表自动刷新
- **Tool Use 支持**：AI 可自主调用工具，实现技能的创建、修改、删除
- **全局提示词**：保存一次个人信息，自动注入每次对话

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

1. 设置 → 模型来源 → `API (云端)`
2. 选择 API 类型：`OpenAI` / `Claude` / `自定义`
3. 填写 API 地址和密钥
4. 推荐模型：`gpt-4o` 或 `claude-3-opus-20240229`

#### 本地 Ollama

```bash
ollama pull llava
```

然后在设置中选择 `Ollama (本地)`，地址填 `http://localhost:11434`。

## 使用方法

1. **开始监控**：点击「开始监控」按钮
2. **正常工作**：OpenCowork 在后台默默记录
3. **遇到问题**：收到 AI 主动推送的错误提醒
4. **查询历史**：用自然语言询问任何时间段的操作
5. **使用 Skills**：输入 `/skill-name` 调用特定能力

### 支持的时间表达

- `刚才`、`刚刚` → 最近 5 分钟
- `最近N分钟` → 指定分钟数
- `今天`、`上午`、`下午` → 当天
- `昨天` → 最近 2 天
- `这周`、`本周` → 最近 7 天

## 配置参考

<details>
<summary>截屏配置</summary>

| 设置项 | 说明 | 默认值 |
|--------|------|--------|
| 截屏间隔 | 每次截屏的间隔时间 | 1000ms |
| 压缩质量 | 截图压缩质量 (10-100) | 80% |
| 跳过无变化 | 画面无变化时跳过识别 | 开启 |
| 变化敏感度 | 相似度阈值 (0.5-0.99) | 0.95 |

</details>

<details>
<summary>错误提醒配置</summary>

| 设置项 | 说明 | 默认值 |
|--------|------|--------|
| 提醒置信度阈值 | 只有置信度超过此值的错误才会提醒 | 0.7 |
| 提醒冷却时间 | 相同错误的提醒间隔（秒） | 120 |

</details>

<details>
<summary>存储配置</summary>

| 设置项 | 说明 | 默认值 |
|--------|------|--------|
| 保留天数 | 历史数据保留时间 | 7 天 |
| 上下文大小 | 对话时加载的最大字符数 | 10000 字符 |

</details>

## 数据存储

```
Windows: %LOCALAPPDATA%\opencowork\data\
macOS:   ~/Library/Application Support/opencowork/data/
Linux:   ~/.local/share/opencowork/data/
```

**隐私保障**：
- 所有数据仅存储在本地
- 截图会保存在本地用于分析，可按保留策略清理
- API 调用时图片会发送到对应的 AI 服务商

## 常见问题

<details>
<summary>Token 消耗太快怎么办？</summary>

1. 确保「跳过无变化」功能已启用（默认开启）
2. 提高「变化敏感度」数值
3. 适当增加截屏间隔
4. 使用本地 Ollama 模型

</details>

<details>
<summary>如何使用国内 API？</summary>

在 API 地址中填写兼容 OpenAI 格式的服务商地址，如：
- 智谱 AI：`https://open.bigmodel.cn/api/paas/v4`
- 通义千问：参考阿里云文档

</details>

## 构建

```bash
# 开发模式
npm run tauri dev

# 生产构建
npm run tauri build
```

## License

MIT
