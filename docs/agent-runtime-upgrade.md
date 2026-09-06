# 通用智能体四批升级

本次修改把对话执行迁入后台任务，并补上上下文、进程、恢复和外部工具的执行链路。旧配置可以继续读取，新字段使用默认值。

| 批次 | 已实现 | 主要位置 |
| --- | --- | --- |
| 1：上下文与续写 | 估算文本、工具参数、调用标识和工具定义开销；预留输出预算；按完整工具调用组压缩；语义总结保留用户原始约束；大结果归档；根据服务端截断标志续写；截断的工具参数不执行 | `commands/history.rs`、`model/api.rs` |
| 2：命令执行 | 管理前台和后台进程，返回进程 ID、状态和输出文件；停止、超时和任务结束清理进程树；Windows Job Object、Unix process group；按参数前缀校验命令白名单 | `commands/process.rs`、`commands/permissions.rs` |
| 3：持久任务 | 调用前记录意图，调用后记录结果；按会话路由回复；切换会话继续执行；重启后标记待恢复；恢复时携带历史和未知结果提示；步骤、成果、验证依据及文本/图片预览 | `commands/tasks.rs`、`stores/agentTasks.ts`、`Chat/TaskPanel.vue` |
| 4：外部工具与并发 | stdio MCP 初始化、工具发现、分页、调用、权限校验、取消后重连；Playwright MCP 浏览器导航/快照/输入/点击/截图；独立文件读取最多并发 8 个，写入和外部动作按顺序执行 | `commands/mcp.rs`、`commands/browser.rs`、`commands/mod.rs` |

## 使用方式

在设置的配置编辑器里调整单次输出上限、上下文容量、并行读取数量和工具权限。默认单次输出上限为 8192，默认并行读取数量为 4。上下文容量应大于输出预算、工具定义和当前输入之和。

发送消息后，可以在“任务与成果”面板查看步骤和文件。切换会话或进入设置不会取消任务；“停止任务”会停止本次执行及所属进程。应用重启后，原来的运行任务显示“待恢复”，点击“检查现场并继续”会创建一次携带原历史的新执行。

恢复不会自动重放旧工具调用。尚未获得结果的调用会标记为“结果未知”，要求模型先检查现场。但外部服务通常没有事务或幂等保证，不能保证模型重试时绝不会重复产生副作用。

“本轮结束”表示模型结束了本轮，并不代表所有成果都已验证。“验证依据”单独标注为助手报告的信息，未记录依据时明确显示未验证。

## 命令权限

白名单规则是完整参数的前缀，例如：

```text
git status
git diff
python "D:\work\trusted-script.py"
node "D:\work\trusted-script.js"
```

白名单模式直接启动程序，不通过 shell 展开。拒绝复合命令、环境变量展开、shell 启动器、脚本求值参数和 `.cmd`/`.bat`。Windows 上的 `npm.cmd` 等启动脚本需要改用对应 `node.exe` 加 JS 入口；需要任意 shell 语法时使用已有的完整权限模式。

目录规则适用于内置文件工具，并解析已存在路径的符号链接。它不是命令或 MCP 服务的操作系统沙箱；配置的程序仍具有当前用户权限。上下文归档目录只对内置 Read 额外开放，不因此开放写入。

后台命令通过 `Bash` 的 `background: true` 启动，通过 `Process` 的 `poll`/`stop` 操作管理。后台命令属于本次任务，任务结束会清理；需要长期驻留的服务应在应用外单独管理。

## MCP 和浏览器配置

目前支持本地 **stdio**，尚未实现 HTTP/SSE、OAuth、MCP resources/prompts 或服务端 sampling。未启用的服务不会启动；白名单模式只允许该服务 `allowed_tools` 列出的工具。测试连接使用已保存并应用的配置。

浏览器实测使用 `@playwright/mcp@0.0.80`、Node 24 和本机 Microsoft Edge。可以将服务安装到自己选择的目录：

```powershell
npm install --prefix D:\tools\opencowork-mcp @playwright/mcp@0.0.80
```

在 MCP 服务配置框填写 JSON，例如：

```json
[
  {
    "name": "browser",
    "enabled": true,
    "command": "node.exe",
    "args": [
      "D:\\tools\\opencowork-mcp\\node_modules\\@playwright\\mcp\\cli.js",
      "--headless",
      "--isolated",
      "--browser",
      "msedge"
    ],
    "env": {},
    "allowed_tools": [
      "browser_navigate",
      "browser_snapshot",
      "browser_click",
      "browser_type",
      "browser_take_screenshot"
    ]
  }
]
```

再把“浏览器 MCP 服务”设为 `browser`，保存并应用，测试连接。浏览器仅允许通过 Browser 导航到 HTTP(S) URL；点击和输入使用最新快照中的元素引用。适配器根据工具定义兼容旧版 `ref` 与新版 `target` 参数。新版导航可能只返回快照文件链接，需要再调用 snapshot 获得页面结构。截图会保存为成果文件并可在任务面板预览。

## 数据与边界

- `data/agent-tasks/*.json` 保存任务输入、调用记录、步骤、成果路径、验证依据和最终回复，临时文件写完后替换原记录。
- `data/agent-context/*.json` 保存压缩前上下文；工作目录的 `.task_outputs/` 保存命令完整日志、大工具输出和 MCP 图片。
- 上下文数量是启发式估算，不是各模型的精确 tokenizer；图片及服务端隐藏开销也可能触发实际超限。只在明确的上下文超限错误时尝试进一步压缩，其他 400 错误直接保留原因。
- 用户约束或单条输入本身超出预算时会报错，不静默丢弃要求。摘要失败或截断时保留原始记录并停止本次请求。
- 文字输出截断最多自动续写 3 次；工具循环达到上限或连续重复失败后任务显示失败，可检查后恢复。
- 工具循环仍使用已有 API provider 路径；Ollama 原有文本路径没有在此次扩展成工具协议。
- 本批未实现多智能体调度、跨任务自动记忆学习或自动重启后执行。这些没有混入当前恢复逻辑。

## 验证与复现

本地验证日期：2026-09-06，Windows。

验证结果：前端类型检查和生产构建通过；`cargo check`、`cargo clippy` 通过但仍有警告；23 项常规 Rust 测试及 1 项真实浏览器测试已通过。最后补充的同提示词多图片测试，与上下文及续写测试一起再次通过。界面模拟 IPC 检查通过。没有执行真实模型账户的桌面端验收。

```powershell
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml
```

常规 Rust 测试使用本地 HTTP/stdio fixture，不需要模型密钥。Node 是 MCP 和进程树测试的前提。覆盖了截断后不重复追加文件、截断工具参数拒绝执行、连续压缩保留约束、图片还原不覆盖工具压缩、读取顺序及写入边界、任务日志恢复、MCP 权限与取消重连、Windows 子进程清理和超时。

真实浏览器测试默认忽略，需要显式提供安装好的 Playwright MCP：

```powershell
$env:OPENCOWORK_PLAYWRIGHT_MCP = 'D:\tools\opencowork-mcp\node_modules\@playwright\mcp\cli.js'
cargo test --manifest-path src-tauri/Cargo.toml test_real_playwright_browser_workflow -- --ignored --nocapture
```

该测试打开本地页面，输入 OpenCowork、点击按钮，检查页面出现 `Hello OpenCowork` 并获取截图。本机实测已通过。

界面检查脚本使用隔离的 Edge 浏览器和模拟 Tauri IPC，不读取用户配置：

```powershell
# 一个终端运行前端
npm run dev
# 另一个终端指定已安装的 playwright 包目录
$env:OPENCOWORK_PLAYWRIGHT_DIR = 'D:\tools\ui-test\node_modules\playwright'
node scripts/check-agent-ui.cjs
```

已通过的界面步骤：启动两个不同会话的任务；在第二会话等待第一任务结束并检查结果归属；切回第一会话预览成果；停止和恢复第二任务；重载页面检查待恢复状态及回复去重。HTML 预览按纯文本处理。该检查不替代带真实模型配置的 Tauri 桌面端最终验收；本次没有调用用户的真实模型账户。

前端还修复了原有 `vue-tsc@1.8` 与当前 TypeScript 不兼容的问题，以及升级检查工具后暴露的 Pinia 解包、过期 Marked 配置和未使用导入错误。
