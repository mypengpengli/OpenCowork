# OpenCowork v0.3.0 - SKILL 编排层优化说明

## 背景

本次优化聚焦于两个核心问题：

1. SKILL 能被发现，但执行阶段容易偏离 `SKILL.md` 指令。
2. 会话切换和工具上下文管理不稳定，导致历史污染、400 请求错误、后台过程残留。

## 本次改动

### 1. SKILL 发现与解析兼容性

- 同时支持 `SKILL.md` 与 `skill.md`。
- 解析 frontmatter 时增强兼容：
  - `allowed-tools` 支持字符串与列表格式。
  - 支持 `disable-model-invocation`。
  - 元数据 `name` 与目录名不一致时，发现阶段不再直接丢弃，做规范化处理。

涉及文件：

- `src-tauri/src/skills/mod.rs`
- `src-tauri/src/skills/parser.rs`

### 2. 技能执行上下文隔离（防串话）

- 技能执行时默认不带之前聊天历史（`model_history = None`），避免旧上下文干扰当前 skill。
- `context` 策略明确化：
  - 仅 `context: screen` 时加载屏幕上下文；
  - 其他情况默认不加载屏幕上下文。
- Slash 参数注入增强：支持 `$ARGUMENTS`、`$ARGUMENTS[n]`、`$1/$2...`。

涉及文件：

- `src-tauri/src/commands/mod.rs`

### 3. 工具与技能调用边界收敛

- 技能执行默认采用最小工具集（未显式声明 `allowed_tools` 时），减少无关工具漂移。
- `manage_skill` / `invoke_skill` / `progress_update` 受 `allowed_tools` 过滤。
- `disable_model_invocation: true` 的技能不会进入模型侧自动调用列表，仅允许手动 `/skill` 调用。

涉及文件：

- `src-tauri/src/model/api.rs`
- `src-tauri/src/commands/mod.rs`

### 4. “有依据执行”机制（关键）

- Skill 模式下，`Bash/run_command` 每次执行前会进行依据校验：
  - 命令需能在当前 `SKILL.md` 的命令片段（代码块/行内命令/URL）中找到依据；
  - 无依据则拒绝执行并返回 `TOOL_ERROR`，引导模型先回到 skill 指令。

涉及文件：

- `src-tauri/src/commands/mod.rs`

### 5. 会话与后台过程状态修复

- 前端不再把 `toolContext` 注入下一轮模型 history（避免 `role=tool` 污染上行请求）。
- 切换会话/新建会话时，重置后台过程面板状态并取消活动请求。
- 新增 `conversationVersion` 用于可靠感知会话切换并触发 UI 状态重置。
- 后端再次防御：history 仅允许 `system/user/assistant` 角色，过滤异常角色，避免 API 400。

涉及文件：

- `src/views/MainView.vue`
- `src/stores/chat.ts`
- `src-tauri/src/model/api.rs`

## 结果

与优化前相比，`/oa` 等技能执行表现为：

- 更稳定地按 `SKILL.md` 流程推进；
- 明显减少“先乱试命令再回头”的行为；
- 会话切换后不再带出上一会话后台过程；
- 显著降低上下文污染导致的 `400 Improperly formed request`。

## 已知边界

- 模型仍可能在复杂失败场景下表现“笨拙”；本次通过编排层约束已大幅收敛，但不等于 100% 无偏差。
- 若技能文档本身含糊或命令依据不足，依据校验会触发拒绝执行，需要补齐 `SKILL.md` 的可执行命令片段。

## 后续建议（未在本次实现）

1. 增加“技能执行前摘要卡片”，向用户展示将执行的依据命令。
2. 对 `tool_result` 长输出做更严格持久化分流（文件引用 + 截断摘要），进一步降低上下文膨胀。
3. 增加“技能执行回放日志”视图，按 step 显示“指令来源 -> 实际命令 -> 返回结果”。
