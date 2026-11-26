# OpenCoWork

一个以本地 Rust 运行时为核心的 AI 工作助手。通过 Windows 桌面应用或本地网页对话，读取和修改项目文件、操作电脑与浏览器、持续执行任务，并审阅更改、管理记忆和复用经验。

**电脑控制和浏览器控制已内置，默认开启。** 不需要安装浏览器扩展，也不依赖智谱、Codex 或 Hermes 的电脑控制服务。模型请求使用你自己配置的 Provider API；截图识别需要支持图像输入的模型。

[快速开始](#快速开始) · [功能与入口](#功能与入口) · [默认设置](#默认设置) · [使用边界](#使用边界) · [开发与验证](#开发与验证)

## 主要功能

- **自主助手（AS 分支）**：从 Windows 桌面版打开「自主助手」并开始观察。关掉主窗口后可从系统托盘重新打开或暂停。支持跨窗口归并事项、静默预测、托盘提示、按授权办理网页和代发三局通文字、核对结果及自动整理 Skill 草稿。可分别配置文字、图片、决策和 Skill 模型。详情见 [AS 开发计划](AS-PLAN.md)。

- **回答与历史**：助手回答支持安全 Markdown、表格、代码高亮与复制；文件链接可定位到行。“会话”页直接全文搜索正文、工具结果和归档，并跳到匹配消息。
- **模型响应与长上下文**：流式保存思考、正文、工具参数和结束原因；断流保留部分内容，截断的工具调用不执行。每次模型请求前检查上下文预算，必要时生成摘要并归档原文。
- **并行子任务**：模型通过 `Subagents` 创建、查看、等待、补充要求、取消和重试任务。默认并发 6 个，超额排队，设置中可调整数量与委派深度。
- **配置与同步**：Provider 页面增加开始使用清单与四项能力检测；团队记忆支持令牌环境变量／文件、冲突双方审阅、大小限制和工作区隔离。
- **本地自动操作**：Windows 窗口与无障碍控件操作、窗口截图、电脑控制开关；独立浏览器的页面读取、表单操作、截图及诊断。
- **持续任务与恢复**：目标模式、步骤与完成证据、暂停续跑、运行中补充要求、消息队列，以及宿主中断后的检查点恢复。
- **项目审阅**：文件和历史会话引用、上下文用量说明、Git 文件／分块暂存与撤销、恢复点，以及独立 worktree。
- **记忆与经验**：中文历史全文搜索、记忆来源与冲突处理、可审阅的技能候选、定时任务与本地结果收件箱。
- **界面与交互**：固定底部输入框，左下角“+”集中提供附件与工具；权限、模型、推理强度和发送／停止位于同一工具栏。命令、工具结果和中间说明合并到默认收起的“执行过程”按钮，展开后在限高面板内上下滚动，最终回答完整展示；侧栏可按项目或时间分类。已修复草稿丢失、切换会话残留任务状态、语言切换影响表单、设置保存错误提示及重复提交等问题。

## 快速开始

### 从源码启动（Windows）

需要 Rust 工具链、MSVC C++ 编译环境，以及桌面窗口使用的 WebView2 Runtime。首次构建会下载并编译依赖。

1. 克隆仓库并进入目录：

   ```powershell
   git clone https://github.com/mypengpengli/OpenCowork.git
   cd OpenCowork
   ```

2. 双击 `Start-OpenClaw.bat`，或在 PowerShell 中运行：

   ```powershell
   .\Start-OpenClaw.bat
   ```

3. 在 **设置 → Provider** 配置服务地址、API Key 和模型，然后发送任务。需要根据截图操作电脑或网页时，选择支持图像输入的模型。

启动器优先打开桌面应用；桌面程序不可用时回退到本地网页 `http://127.0.0.1:33211/`。首次启动或构建缓存来自其他项目副本时会构建，日常启动直接运行已有程序。

**拉取更新后需要重新构建，才能看到新功能和界面：**

```powershell
git pull --ff-only
.\Start-OpenClaw.bat --build
```

也可以只运行 `.\Build-OpenCowork.bat` 构建。启动器和下文开发命令共用 `%LOCALAPPDATA%\OpenClaw\target`；普通启动不会检查源码是否改变。

### 安装 Windows 客户端

从 [Release 下载 Windows 安装程序](https://github.com/mypengpengli/OpenCowork/releases/download/v0.6.0/OpenCowork-0.6.0-Setup-x64.exe)，双击 `OpenCowork-0.6.0-Setup-x64.exe` 完成安装，然后从桌面或开始菜单打开 OpenCowork。Release 只提供这一个安装文件。

安装到当前用户账户，不需要管理员权限。已有 WebView2 时直接使用；缺少时由安装程序补齐，需要联网。可在 Windows「已安装的应用」中卸载；升级和卸载保留工作区、会话与模型设置。

- 桌面运行需要 Windows 10／11 x64 和 WebView2；无需安装 Rust、MSVC 或 Node.js。
- 内置浏览器工具还需要本机安装 Chrome 或 Edge；无需浏览器扩展。
- Git 审阅、暂存和隔离工作区功能需要 Git。自行配置的 MCP 服务可能需要额外运行时。
- 启动失败会显示错误对话框，日志位于 `%LOCALAPPDATA%\OpenCowork\logs`。

安装包由 [安装打包脚本](scripts/package-installer.ps1) 生成，包含桌面与网页宿主、快捷方式、卸载程序和 WebView2 引导程序。源码更新后需要重新构建安装包。

开发者构建安装包需安装 [NSIS](https://nsis.sourceforge.io/Download)，并下载微软官方 [WebView2 Evergreen Bootstrapper](https://developer.microsoft.com/microsoft-edge/webview2/)。运行 `scripts/package-installer.ps1 -NsisCompiler <makensis.exe路径> -WebViewBootstrapper <引导程序路径>`；脚本检查微软签名、构建优化版并生成 `dist/installers/OpenCowork-0.6.0-Setup-x64.exe`。安装与升级回归可运行 `python scripts/check-installer.py dist/installers/OpenCowork-0.6.0-Setup-x64.exe`。

## 工作区与会话

新会话中，点击输入框上方的 **工作区名称 → 打开文件夹…**，浏览磁盘和子文件夹，或输入完整路径，再选择“在此文件夹中工作”。最近打开的工作区支持搜索和再次打开；无需通过启动目录来切换项目。无会话项目在侧栏显示“×”，最近列表也提供移除按钮：只移除最近记录，保留磁盘文件、历史会话和已有页面的工作区标识，重启不会自动恢复已移除记录；当前项目需先切换后再移除。

已有对话的会话名称和工作区显示在左上方，底部不再提供目录切换。点击顶部菜单可选择“在资源管理器中打开”或“重命名会话”；重命名只改变会话标题，保留目录与消息，刷新、继续对话和压缩历史后仍生效。运行中的任务需先停止再重命名。需要换项目时先新建会话。

侧栏默认按 **项目** 展示会话，也可以切到 **时间** 分类。项目可折叠，点击项目旁的箭头可在其中新建会话；打开其他项目的会话会先切换到它所属的目录。旧版未记录项目的会话单独列为“未归属项目”。

文件、设置和后续工具执行跟随所选工作区。各页面使用独立的工作区上下文；切换前须停止当前页面正在执行的任务。未发送文字草稿按会话／工作区保存。最近工作区保存在用户配置目录的 `workspaces.json`；不会复制其他项目的密钥或本地设置，新项目需要可用的用户级或项目级模型配置。

## 功能与入口

对话输入框固定在底部。左下角 **+** 提供附件与工具菜单，一次打开一个面板；点击收起、面板外区域或按 `Esc` 关闭。模型、权限和推理强度可在输入框底部调整，发送和停止共用同一位置。上方保留当前工作区、任务步骤、目标状态和排队消息提示。顶部左侧显示当前会话与工作区，右侧保留“会话概览”，模型、权限、配置保存状态、更新时间和运行辅助集中在概览中；“团队记忆同步：未配置”仅表示可选同步服务未设置。

输入 `/` 选择命令：技能、MCP 服务和工具默认选入输入框，补充任务后发送即可使用；每项独立的“查看”按钮打开说明或设置。`Tab` 只补全，`Enter` 使用选中项或提交完整命令。`/permissions 模式` 会保存权限设置，`/compact` 会压缩并保存当前会话，保留最近 6 条消息及其关联工具调用；较早原文归档后仍可全文搜索。执行结果会显示压缩数量和上下文消息数变化；不足以压缩时明确提示“无需压缩”，任务运行中需先停止再执行。`/settings`、`/history` 等标有“导航”的命令仍用于打开相应页面。

| 功能 | 界面入口 | 可以做什么 |
| --- | --- | --- |
| Windows 电脑控制 | 直接在对话中提出任务；设置 → 权限 | 选择窗口、读取无障碍控件、截图、点击、拖动、双向滚动、输入中文和组合键；聊天中预览截图 |
| 电脑控制开关 | 对话工具栏“电脑控制：已启用／未启用” | 点击切换并保存项目设置；关闭时取消当前宿主的活动任务，重新启用不自动重跑任务 |
| 内置浏览器 | 对话；设置 → 权限中的浏览器开关 | 启动独立浏览器，打开网页、读取页面、填写表单、点击、等待、调整尺寸、截图和查看控制台／网络诊断 |
| 文件与引用 | + → 添加文件 | 浏览文件、预览文本和 Git 差异，将文件或历史会话加入本次上下文 |
| 更改审阅与恢复 | + → 审阅更改与恢复 | 按文件或分块暂存、取消暂存、撤销未暂存文本更改；查看恢复点，拒绝过期审阅版本 |
| 隔离工作区 | + → 隔离工作区 | 从已提交的 Git HEAD 创建独立 worktree 和分支，查看已有工作区 |
| 任务进度 | + → 任务步骤 | 查看持久化步骤、检查项和完成证据，继续中断的工作 |
| 子智能体 | + → 子智能体任务；设置 → 权限 → 子智能体 | 查看排队、运行、结果和失败原因，取消任务，调整并发数与深度 |
| 持续目标与消息队列 | + → 目标、补充要求与消息队列 | 设置目标、暂停和恢复；运行中补充要求，或把消息排入下一轮 |
| 定时任务 | 左侧 → 定时任务 | 新建、编辑、暂停、删除或立即执行任务，查看下次执行时间与执行记录 |
| 上下文诊断 | + → 本次上下文来源与用量 | 查看提示层、来源、纳入原因、引用截断和估算用量 |
| 中文全文搜索 | 会话页搜索框；+ → 历史全文搜索 | 搜索所有项目的消息正文、工具结果与归档；点击结果定位消息，归档打开原文 |
| 回答排版 | 助手回复 | 标题、列表、表格和代码高亮；复制代码；点击文件链接查看对应行 |
| 团队记忆同步 | 设置 → 记忆 → 团队记忆同步 | 配置地址和认证；审阅冲突后选择本地或远端；更改同步配置后重启应用 |
| 记忆管理 | + → 记忆来源与冲突 | 查看自动事实的来源和时间，对同标题冲突选择保留或替换 |
| 技能复用 | + → 可复用技能候选 | 对照旧版与候选的步骤、完成检查、失败恢复和证据，采用、拒绝或停用 |
| 模型与扩展 | 设置中的 Provider、权限、Skills、MCP | 管理模型配置、执行预算、功能开关、技能和 MCP 扩展 |

首次配置可以从 **设置 → Provider → 开始使用** 查看工作区、当前模型、凭据是否存在和 MCP 数量。“已找到凭据”不等于连接成功；点击检测会发出最多四次小请求，分别检查连接、工具、图像和流式能力。MCP 和额外技能均为可选项。

Provider 配置档还可指定上下文窗口、最大输出、图片输入能力，以及接口是否要求回传 `reasoning_content`。默认开启流式输出；思考字段由接口实际返回时才显示和保存。思考内容按 Provider 地址、标签及模型归属回传，切换接口或模型后不混用历史思考字段。

Markdown 使用受限的本地 DOM 渲染，不执行模型返回的 HTML，也不自动加载远端图片。文件链接只能打开当前工作区中的文本文件；外部网页链接由用户点击打开。

### 电脑与浏览器怎样工作

**电脑控制**通过本地 Windows API 操作真实桌面。输入绑定选定窗口及最新观察状态，操作后返回新状态；窗口截图使用 Windows Graphics Capture，并在需要时报告兼容回退。每个聊天工作进程复用自己的电脑助手。输入与用户共享活动桌面，可能受窗口焦点和系统权限限制；最小化、受保护或部分硬件加速窗口可能无法截图。已经交给 Windows 的输入不会因停止任务而撤回。

**浏览器控制是软件内置能力。** 它通过 CDP 启动本机 Chrome／Edge 的独立无头实例，使用独立配置目录，不导入你日常浏览器的账号登录状态。它与操作真实 Windows 窗口的电脑工具是两种不同入口，可根据任务选择。

### 长任务、停止与恢复

对话实时显示模型文本和工具事件。点击停止会取消当前模型请求及其拥有的子进程，保存已收到的内容。规划、恢复和处理队列时会保留未发送草稿；已有草稿时，合并任务要求后等待你检查并发送。

持续目标要求任务步骤和实际工具证据，再通过独立模型请求检查完成情况。默认最多续跑 12 轮、500000 tokens；暂停、预算耗尽或异常后保留进度。

工具意图、结果和补充消息经过落盘确认，文本增量定期保存检查点。宿主意外退出后，重启可恢复已保存内容；没有收到结果的动作会标记为“结果未知”，中断目标保持暂停，避免盲目重放已发生的操作。

每个模型请求都会计算系统提示、工具定义、历史、引用和截图预留，先使用工具结果的落盘预览，再按预算压缩较早消息。语义摘要保留用户约束、修正、已完成证据和下一步，摘要失败时保留原上下文。上下文溢出最多压缩重试一次，不重放已执行的工具；原文仍可在历史归档中查阅。

子任务默认使用独立上下文和只读权限，主智能体不计入默认 6 个并发名额。主任务停止时会停止关联子任务；宿主重启后遗留任务标记为中断，保留会话与结果，可在任务面板检查后直接恢复到新任务组。面板支持手工创建、补充要求、继承当前会话、单独选择模型与推理强度，展示耗时、工具证据和受影响文件。`writable=true` 需要父任务具有写权限，写入及桌面操作使用配置目录级互斥锁协调。默认最大深度为 1，调整为 2 或更高后，子任务等待后代时会释放执行名额。共享预算按返回的 usage 核算，在途请求可能消耗剩余额度。Provider 的「共享并发请求上限」通过跨进程请求槽位限制同一地址，默认 64；多个项目采用相同上限时共同遵守该额度。

模型可通过 `Process` 启动后台命令或原生 PTY，立即取得会话 ID，随后按字节游标读取输出、等待指定输出标记、发送输入、发送 Ctrl-C、调整终端大小、查询退出码或取消。Windows PTY 基于 [portable-pty](https://docs.rs/portable-pty/0.9.0/portable_pty/) 的 ConPTY 后端；日志保留上限为 16 MiB，达到上限后继续排空输出，避免堵塞子进程。「读取等待超时」与「进程退出」分别报告。进程归属根任务和创建它的子任务，任务停止或宿主退出后清理进程树，重启后保留中断记录。任务面板可查看日志与取消进程。

「本次上下文来源与用量」展示每次请求的首事件、首正文和总耗时，以及压缩前后估算、压缩耗时和摘要用量。文本估算区分 ASCII、中文及其他 Unicode 字符，优先校准到服务商返回的真实 usage；估算仍可能与模型 tokenizer 不同。

### 文件、记忆和经验

文件／会话引用最多 8 项，每项 12000 字符、总计 24000 字符，界面显示截断及估算 token 用量，引用随用户消息保存。Git 审阅包含暂存区与工作区差异，写入前检查内容版本，并为恢复操作保存原内容。

前台记忆检索采用有界文本匹配，最多选取 3 条／6000 字符，不为挑选记忆额外调用模型。后台记忆提取和技能候选生成需在设置中开启；候选附带来源证据并支持人工审阅，采用后写入项目的 `.opencowork/skills/learned-*/SKILL.md`。

### 团队记忆同步（可选）

在 **设置 → 记忆 → 团队记忆同步** 配置 HTTP(S) 服务地址、仓库标识，以及令牌环境变量或本地令牌文件（完整路径，最多 16 KB）。更改后重启应用生效。指定的环境变量缺失或认证失败会报错，并保留本地内容；未配置同步不影响日常使用。

每个工作区独立同步。双方修改同一文件时暂停上传，在“查看同步冲突”中对比并选择保留本地或使用远端。需要手工合并时先编辑本地文件，刷新后再保留本地；目前不传播删除操作。

需要自行提供兼容的同步服务，支持 GET/PUT、ETag/checksum 和 If-Match／If-None-Match 条件请求，204 响应需返回 ETag。单文件最多 250 KB，每次最多 512 项，请求／响应最多 4 MiB；首次创建使用 If-None-Match: *。

## 默认设置

在 **设置 → 权限** 调整功能开关和执行预算，实际执行仍受当前工具权限设置约束。项目配置使用 `.opencowork/settings.json`，并与用户级设置合并。

| 设置 | 默认值 |
| --- | --- |
| 电脑控制 `computer.enabled` | 开启 |
| 浏览器控制 `browser.enabled` | 开启 |
| 后台记忆提取 `memory.backgroundEnabled` | 关闭 |
| 技能候选生成 `learning.enabled` | 关闭 |
| 技能自动采用 `learning.autoAdopt` | 关闭 |
| 模型迭代上限 `execution.maxIterations` | 0（不限；正整数设置上限） |
| 单段自动续跑 token 阈值 | 250000 |
| 单段自动续跑时间 | 900 秒 |
| 连续无进展停止阈值 | 3 |
| 单次输出 token 上限 | 8192 |
| Provider 临时错误重试次数 | 2（可在 Provider 配置档设为 0–5） |

可指定同一 Provider 下的独立后台模型名称，以及可选推理强度。模型诊断最多发起 4 次小请求，检查连接、工具调用、图像输入和流式返回，并显示耗时及用量；这些请求可能产生 API 费用。

Provider 在连接失败、请求超时、HTTP 408/409/429 和 5xx（包括 504）时按指数退避自动重试，并遵循服务端 `Retry-After` 和 `x-should-retry` 指示。默认重试 2 次；鉴权、参数等确定性错误不会重试。响应流一旦开始，流中断不会自动重放，避免重复生成内容或再次执行工具。重试耗尽后，会话中的系统错误卡提供“继续任务”，让智能体基于已保存的工具结果继续，而不是盲目重复整轮操作。

桌面对话达到单段时间或 token 阈值时，OpenCowork 会配对尚未完成的工具调用、保存会话并自动开启下一执行段。模型迭代次数默认不限；可在设置中填写正整数设置任务上限，已有的显式上限继续生效。连续 3 个执行段没有成功工具结果时停止，防止无效循环。工具是否已经执行不明确时，续跑提示会要求先检查当前状态，避免重复提交或重复写入。停止按钮始终可用；目标模式仍保留独立的目标续轮预算。

普通输入与缓存用量分开统计，不重复相加。服务端缺少 usage 时无法精确执行 token 上限，时间与迭代限制仍有效；单次模型调用可能超过剩余 token 预算。未知价格不推算费用。

## 使用边界

- **本地运行不等于离线推理**：模型请求会发往你配置的 API；任务中使用的文件文本、截图等可能作为上下文发送给该服务。
- **权限范围**：电脑操作发生在当前 Windows 活动桌面。关闭电脑控制会取消活动对话并禁用该工具，但不会限制通用 shell。文件工具的工作区写入限制也不是 shell、MCP 或网络访问的操作系统沙箱。
- **浏览器范围**：跨回合登录态持久化、跨进程 iframe、文件上传和复杂站点登录尚未纳入当前验收范围。
- **Git 分块范围**：分块操作支持普通现有文本文件，暂不处理二进制、新增、删除或重命名文件；完整文件暂存支持新增文件。worktree 不自动复制本地配置、安装依赖或合并分支。
- **定时任务范围**：需要 OpenCoWork 宿主保持运行，结果投递到本地收件箱；不支持关机后执行。异常或执行结果未知的任务暂停，不自动重放。
- **模型协议**：当前使用 OpenAI 兼容的 Chat Completions 工具／图像协议，尚未实现原生 Responses 或 Anthropic Messages。
- **验证范围**：已有本地替身服务回归、原生窗口测试及真实模型浏览器表单完整流程实测；更广泛的任务成功率与全新机器安装仍需进一步实测。

## 开发与验证

主入口为 `opencowork-desktop` 和 `opencowork-shell`，CLI 用于调试与底层运行时操作。当前实现已迁移到 Rust，不使用旧版 Vite 前端启动流程。

```text
crates/
├── api                  # 模型请求与流类型
├── app                  # 独立于 UI 的运行时和事件层
├── runtime              # 会话、上下文压缩、配置、权限、记忆与执行循环
├── tools                # 文件、shell、电脑、浏览器等内置工具
├── plugins              # 插件清单、安装与启用状态
├── agents               # 角色、任务交接与调度
├── skills               # 技能发现与元数据
├── mcp                  # MCP 传输、工具、资源与认证
├── lsp                  # 按需启动的语义上下文
├── commands             # 斜杠命令
├── opencowork-cli        # 命令行入口
├── opencowork-shell      # 本地网页与设置界面
└── opencowork-desktop    # Windows WebView 桌面宿主
```

运行时保留分层指令加载、项目／会话记忆、上下文压缩、延迟工具发现、按需 LSP、插件 hooks 与持久化任务交接。MCP 提供 stdio、HTTP、SSE、WebSocket 传输，以及资源读取和认证配置。

```powershell
# 启动或构建
cargo run --target-dir "$env:LOCALAPPDATA\OpenClaw\target" -p opencowork-shell
cargo run --target-dir "$env:LOCALAPPDATA\OpenClaw\target" -p opencowork-desktop
cargo run --target-dir "$env:LOCALAPPDATA\OpenClaw\target" -p opencowork-cli -- provider

# 格式和 Rust 测试
cargo fmt --check
cargo test --target-dir "$env:LOCALAPPDATA\OpenClaw\target"

# 界面、流式解析与隔离集成回归
node scripts/check-ui-controls.mjs
node scripts/check-chat-stream.mjs
node scripts/check-markdown.mjs
python scripts/check-workspaces.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe"
python scripts/check-launcher.py
python scripts/check-startup.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe"
python scripts/check-chat.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe"
python scripts/check-features.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe"
python scripts/check-runtime-upgrades.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe"
# 可选界面回归需要 Python Playwright 和本机 Edge
python scripts/check-runtime-ui.py
python scripts/check-setup-sync.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe"
python scripts/check-optimizations.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe"

# 原生窗口测试：需要交互式 Windows 桌面，会操作自己的测试窗口
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-computer-window.ps1

# 生成便携包，默认 release；可追加 -Profile debug
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-windows.ps1
```

Node.js 和 Python 用于上述开发检查，不是桌面应用的启动依赖。隔离集成回归使用临时项目和本地模型替身；浏览器测试还需要 Chrome／Edge。打包输出位于 `dist/packages`，包含 ZIP 与 SHA-256 清单。

流式接口 `POST /api/chat` 返回 NDJSON（`started`、`event`、`complete`）；`POST /api/chat/:turn_id/cancel` 请求取消。会话通过独占租约避免并发写入。内置 `bash` 的默认超时为 120 秒，可通过 `timeoutMs` 设置为 100–600000 毫秒。

真实模型实测中，能力检测、文件读写及编辑回读已通过。2026-09-17 使用当前配置的 `[编程]gpt-5.4` 跑通浏览器表单完整流程：打开一次性本地页面 → 填写 Ada → 点击提交 → 读取页面 Hello Ada → 返回一致的最终回答，耗时 76.3 秒，三次浏览器调用成功，无人工接管或重复调用。该结果覆盖内置浏览器与真实模型的调用链，不代表所有外部网站的兼容性或长期成功率。

复测命令如下（会消耗已配置 API 的用量，报告保存在本地临时目录）：

```powershell
New-Item -ItemType Directory -Force .tmp | Out-Null
python scripts/check-real-provider.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe" --run --case browser-form --output .tmp/browser-real.json
```

仓库保留程序源码、构建与回归脚本，以及 `third-party` 中随依赖分发所需的许可证。构建产物、临时测试报告和本地配置不纳入版本控制。
