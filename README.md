# Screen Assistant - 你的 AI 工作伴侣

[![Version](https://img.shields.io/badge/version-2.0.0-blue.svg)](https://github.com/mypengpengli/screen-assistant)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Stars](https://img.shields.io/github/stars/mypengpengli/screen-assistant?style=social)](https://github.com/mypengpengli/screen-assistant)

> **再也不用担心错过任何一个报错信息了。**

你是否有过这样的经历？

- 部署项目时，终端里一闪而过的红色报错，等你反应过来已经被新日志刷走了
- 调试代码时，错误信息太长，还没来得及复制就被覆盖了
- 想问 AI 帮忙解决问题，却记不清刚才的报错内容是什么
- 一边写代码一边查百度/Google，来回切换窗口打断思路

**Screen Assistant 就是为解决这些痛点而生的。**

## 它能做什么？

### 🎯 自动捕获每一个错误

从你点击「开始监控」的那一刻起，Screen Assistant 就像一个不知疲倦的助手，持续观察你的屏幕。无论是编译错误、运行时异常、还是控制台警告——**每一个细节都会被记录下来**。

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

Screen Assistant 理解自然语言，支持多轮对话，就像和一个了解你所有操作历史的同事聊天一样。

### 🔧 Skills 系统 —— 无限扩展的能力

这是 Screen Assistant 最强大的特性。

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

**对于企业用户**：可以为团队定制专属 Skills，统一工作流程，提升协作效率。

**对于个人开发者**：可以把常用的操作封装成 Skill，一句话完成复杂任务。

### 🧠 智能跳帧，省钱省心

担心 Token 消耗太快？Screen Assistant 使用感知哈希算法对比画面，**当屏幕没有变化时自动跳过分析**，在保证不遗漏任何重要信息的同时，大幅降低 API 调用成本。

## 适用场景

| 场景 | 痛点 | Screen Assistant 的解决方案 |
|------|------|---------------------------|
| **项目部署** | 日志刷屏，错误一闪而过 | 自动捕获并保存所有错误信息 |
| **代码调试** | 报错太长，来不及复制 | 完整记录，随时回溯查询 |
| **学习编程** | 不知道自己哪里做错了 | AI 主动分析错误原因并给出建议 |
| **远程协作** | 难以描述遇到的问题 | 导出操作记录，精准还原问题现场 |
| **工作复盘** | 忘记今天做了什么 | 自然语言查询任意时间段的操作 |

## 技术亮点

- **Tauri 2 + Rust**：原生性能，极低资源占用
- **Vue 3 + TypeScript**：现代化前端，流畅体验
- **双模型支持**：云端 API (OpenAI/Claude) 或本地 Ollama，灵活选择
- **两层存储架构**：原始记录 + 智能聚合，平衡详细度与存储效率
- **隐私优先**：所有数据本地存储，截图不落盘
- **Tool Use 支持**：AI 可自主调用工具，实现技能的创建、修改、删除

## 快速开始

### 环境要求

- Node.js 18+
- Rust 1.70+
- 可选：Ollama（用于本地模型）

### 安装

```bash
git clone https://github.com/mypengpengli/screen-assistant.git
cd screen-assistant
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
2. **正常工作**：Screen Assistant 在后台默默记录
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
Windows: %LOCALAPPDATA%\screen-assistant\data\
macOS:   ~/Library/Application Support/screen-assistant/data/
Linux:   ~/.local/share/screen-assistant/data/
```

**隐私保障**：
- 所有数据仅存储在本地
- 截图不保存到磁盘，仅保存 AI 分析后的文字摘要
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
