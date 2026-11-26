**OpenCowork 模型响应与长任务执行优化计划**

AS 分支的当前开发计划见 [AS 桌面自主助手开发计划](AS-PLAN.md)。下文保留原运行时计划及其验收记录。

记录日期为 2026-11-20，审查基线为 `a1ea22e`。模型响应链路、每步上下文治理和可配置子智能体调度的核心实现已落地。下方分别记录已完成内容和仍待增强的部分，后台进程与交互终端保持后续阶段。

实现顺序为响应完整性 → 每步上下文治理 → 子智能体调度。后台进程与交互终端作为长任务后续阶段。默认数值是本计划的建议，实施时通过设置开放调整。

**一、源码核对与设计依据**

当前项目是独立 Rust 运行时。仓库缺少能定位原始 Pi 对照版本的依赖或版本记录，无法据此判断最初使用的是哪版 Pi。Pi 上游更新需要有选择地移植到 Rust 的协议、会话、执行和前端层。

| 能力 | Pi 当前处理 | OpenCowork 在审查基线中的差距 |
| --- | --- | --- |
| 思考与文字输出 | `AssistantMessageEvent` 分别定义 text、thinking、toolcall 的 start/delta/end，区分 done/error，并记录结束原因 | ProviderEvent、AssistantEvent、AppEvent 和 ContentBlock 缺少完整思考链路与结束原因 |
| 思考内容回传 | OpenAI 兼容适配器识别 reasoning_content/reasoning/reasoning_text，保留回传信息 | 当前请求和会话类型不能完整保存、回传这些信息 |
| 每步上下文检查 | AgentSession 在 prepareNextTurnWithContext 中调用 _compactBeforeNextAssistantResponse | 完整整理主要发生在进入运行时之前，运行时连续工具循环内只限制工具结果体积 |
| 子智能体 | 核心选择通过扩展实现；仓库提供 subagent 扩展示例 | 已有 CLI handoff/worker 和存储锁，缺少桌面对话可直接调用的调度与结果回收链路 |

已核实的 Pi 版本记录如下。

- `0.70.1`，2026-04-24，修复 DeepSeek V4 思考控制与 reasoning_content 回传造成的会话恢复 400。这是特定兼容性修复，不能当作 Pi 首次支持 thinking 的版本。
- `0.84.4`，2026-08-28，修复大工具结果越过压缩阈值后直接进入下一次模型请求，在同一次任务内部加入工具后、模型响应前的压缩。
- `pi-ai 1.3.0`，2026-10-15，修复代理在同一 chunk 返回 reasoning 和正文时的显示顺序。
- `pi-ai 1.5.0`，2026-11-02，修复流式工具参数反复全量解析造成的平方级开销，提示本项目在加入参数增量时应避免每个 chunk 全量重建。

主流并发数字的含义需区分任务数、同时运行数、会话线程数和委派深度。

| 参考实现 | 已核实的限制 | 对本项目的启示 |
| --- | --- | --- |
| Pi 仓库的 subagent 示例扩展 | MAX_PARALLEL_TASKS=8，MAX_CONCURRENCY=4 | 一批任务可多于执行槽位，队列与并发应分别管理；这些数字只属于示例扩展 |
| Codex 当前源码 | 传统后端默认 6，MultiAgentV2 默认 4；当前配置文档明确主线程不计入 spawned-agent 并发限制 | 默认值依后端和版本变化，配置应明确计数口径 |
| Claude Code 当前官方文档 | 普通 Agent 工具默认限制 20 个同时运行的子智能体，可配置；默认允许主会话以下 3 层嵌套 | 并发和深度是两项限制；文档注明该并发机制要求 v2.1.217+，ultracode、恢复及其他 agent 功能另有计数细节 |

以上数字是实现限制，不是经过同任务实测得出的最佳并发值。

**二、阶段 A——模型响应保真与准确展示**

涉及 `crates/api`、`crates/runtime`、`crates/app`、`crates/opencowork-shell/src/chat.rs`、`static/chat-stream.mjs` 和 `static/app.js`。

- [x] 固化 SSE 完整性回归：没有 `[DONE]` 或 `finish_reason` 的 EOF 报中断，已收到内容保留；流内错误不再按成功处理。
- [x] 正常完成、工具交接、输出截断、取消和失败贯穿适配器、运行时、检查点与界面。截断输出中的工具调用不执行。
- [x] 保存独立思考块、正文块、工具参数草稿与结束原因。工具调用完成时在原位置替换草稿，保留块顺序；参数仅在完整后解析与执行。
- [x] 支持 `reasoning_content`、`reasoning`、`reasoning_text`，同一 chunk 先处理思考再处理正文，多个别名避免重复追加。
- [x] 思考按 Provider 标签、实际地址与模型归属保存和回传；跨 Provider、跨模型不混用思考字段。要求 `reasoning_content` 的兼容接口可显式启用空值补全。
- [x] 新增字段采用兼容默认值，旧会话和配置可以加载；PartialTurn、事件序号和恢复检查点覆盖思考与工具草稿。
- [x] 根据实际事件展示等待、思考、正文、准备工具与执行工具；思考可折叠。接口没有返回思考字段时不生成占位思考内容。
- [x] 流式渲染按 50ms 合并更新，只更新当前助手消息中变化的块，保留历史 DOM 与折叠状态；思考及参数草稿检查点按 250ms 合并。
- [x] Provider 配置档支持窗口、输出上限、图片输入及思考回传要求，保存时校验；继续默认开启流式输出。
- [x] 每次请求持久化首事件、首正文、总耗时、状态和用量，在上下文诊断展示；能力检测返回并展示来源及检测时间。

按需协议扩展说明：当前用户指定服务使用 OpenAI Chat Completions 兼容协议。原生 Responses／Anthropic 的签名与不透明状态仅在接入相应原生服务时实施，不属于本次兼容接口的待办，不能将兼容接口字段当作原生协议实现。

验收覆盖纯文字、仅思考后正文、同 chunk 思考与正文、工具调用与回传、碎片 JSON/UTF-8、正常结束、输出上限、无终止证据 EOF、流内错误、取消、旧会话恢复和提供商切换。已开始产生工具副作用的任务在恢复时先检查实际状态。

真实接口验收使用用户指定的 `astron-code-latest` 和指定服务，进行有界的文字及工具任务。实际字段、工具与图片能力以返回结果为准。测试凭据从临时输入或环境读取，报告只保存脱敏结果和统计。

**三、阶段 B——每次请求前的上下文治理**

涉及 `crates/runtime/src/context.rs`、`compact.rs`、`conversation.rs`、`model_context.rs`、`session_memory.rs`、`memory.rs` 以及 `crates/app` 的运行时组装。

- [x] 修复先 snip 后摘要丢失原文的问题，移出消息进入归档，并形成摘要、提交和快照记录。
- [x] App／桌面与 CLI 的每次模型调用前检查上下文，覆盖工具循环、补充要求、续跑和当前模型能力。
- [x] 分开核算窗口占用与任务累计用量。先投影工具结果预览，再检查系统提示、工具 schema、历史、引用、图片预留和输出空间。
- [x] 利用当前压缩版本仍适用的真实 usage，再估算后续消息和新增开销；压缩使旧 usage 失效。缺少 usage 时使用估算。
- [x] 按模型窗口／输出配置确定阈值，近期消息按 token 预算保留，并维护工具调用和结果配对。
- [x] 在摘要前使用工具结果落盘预览；语义摘要分段输入，约束单次摘要请求大小。摘要提示保留目标、修正、约束、证据、文件、待办与下一步。
- [x] 生成和验证通过后才替换活动上下文，归档原消息并增加压缩版本。摘要失败保留原文；单工作进程同步提交，补充消息在下一循环注入。
- [x] 后台记忆状态增加压缩版本；拒绝重新导入旧版本的记忆文件和消息计数，新版本重新提取。
- [x] 上下文溢出单独分类，最多强制压缩重试一次，并验证投影缩小且可容纳；不重放已执行工具。
- [x] 界面事件展示压缩前后估算，结束诊断报告当前窗口占用和压缩状态；原文保留在历史归档与检查点。
- [x] 统一运行时的多语言估算，区分 ASCII／中文／其他 Unicode 并优先采用当前版本真实 usage；窗口另外预留截图容量。压缩提交保存耗时、前后估算与摘要用量，累计用量包含归档和摘要；本地对照场景输出 JSON 报告。估算不等同服务商 tokenizer。

验收使用较小的测试窗口稳定触发至少 3 次压缩，验证首次用户约束、后续修正、未完成任务及关键文件仍能被恢复。另测工具配对、摘要失败/取消、并发补充要求、旧会话、模型切换、缺失 usage 和单次任务多轮工具调用。与修改前记录对照耗时、用量和任务结果。

**四、阶段 C——可配置的子智能体调度**

用户希望子智能体数量可扩展。建议初始默认 `maxConcurrentSubagents=6`，主智能体单独计数，设置页支持自定义正整数，包括 4、8、12、20 等值。并发值受共享预算、Provider 并发额度和宿主资源约束。任务按需要创建，完成后由队列补位。

默认委派深度建议为 1，根为深度 0。深度单独配置，后续启用更深嵌套前先完成父子等待和资源准入的验收。数量与深度不能共用一个限制。

在 `crates/agents::jobs` 增加可持久化任务组与子任务存储，复用运行时的操作系统文件锁、原子写入、ProcessTree 和 AppRuntime，接入工具注册表及桌面 UI。现有 CLI handoff 接口继续保留。

- [x] `Subagents` 提供 create／list／inspect／wait／message／cancel／resume。wait 在工具进程本地等待，最长 30 秒，不在等待过程中循环调用模型。
- [x] 默认并发 6，不包含主智能体。配置为 `subagents.maxConcurrent` 与 `subagents.maxDepth`，设置 → 权限可保存，活动任务组同步调整额度；超出额度排队。
- [x] 持久化 queued／running／waiting／ready／completed／failed／cancelled／interrupted；稳定任务 ID 去重，状态原子更新。
- [x] 子任务使用独立上下文、默认只读，可选择同一 Provider 下的模型；任务说明要求传递用户约束与完成检查。
- [x] 共享当前根任务的用量与时间预算；子进程有自己的剩余预算。预算按返回 usage 核算，在途请求可能超出余额，尚不承诺硬 token 上限。
- [x] 取消主任务停止排队及运行子任务；取消子任务连带停止后代。启动先绑定 ProcessTree 再放行，宿主退出释放子进程所有权；重启后遗留任务标记为中断。
- [x] inspect／wait 返回状态、模型、用量、错误与完整结果，不受 200 字符预览限制；同一活动根任务内可重试失败／取消任务，恢复提示先检查已有副作用。
- [x] 可写子任务需父任务有写权限；写入与桌面操作通过配置目录级互斥锁协调。嵌套可写子任务拒绝准入，避免持有写锁再等待写锁。
- [x] 等待后代的子任务释放执行名额，结束等待后重新排队获得名额；禁止等待自身或祖先，默认深度 1，可配置至 16。
- [x] 桌面显示层级、排队／运行数量、模型、用量、完整结果和错误，可取消任务。
- [x] 调度使用文件事件唤醒并保留超时／退出心跳；同服务地址的请求使用跨进程共享槽位，覆盖主会话、子任务、摘要和能力检测。Provider 可配置共享并发上限，默认 64；同地址的配置应保持一致。
- [x] 跨根任务恢复复制保存检查点并记录来源；显式 fork 记录继承会话并补齐未决工具边界；子任务可独立指定推理强度，持久化开始／结束时间、工具证据和受影响路径。
- [x] UI 支持手工创建、补充要求、取消和检查后恢复，并展示完整结果；补充文本输入期间刷新不替换编辑区域。
- [x] 验收 1／3／4／6／8／12／20 并发、24 任务在 20 名额下排队、跨进程 Provider 限额 2、两个可写任务修改同文件的互斥、单槽嵌套、显式 fork、推理强度及宿主重启后的跨根恢复；Windows 自测窗口验证既有输入、焦点与截图保护。

验收包含默认 6 个并发时提交 12 个任务、切换配置为 3/8/12、部分失败、取消排队及运行任务、重复提交、重启恢复、结果完整取回、冲突写入和共享桌面访问。并发 20 的压力验证使用本地替身 Provider，真实服务按其额度进行有界验证。

阶段比较记录 1/4/6/8 并发的成功率、端到端耗时、输入/输出及缓存用量、重试和冲突次数。默认值可以依据同模型同任务实测调整。

**五、长任务后续——后台进程与交互终端**

- [x] 新增 `Process` 工具：start／list／inspect／read／input／interrupt／resize／cancel／remove。启动返回 ID，日志以字节游标读取，最多保留 16MiB，每次读取最多 64KiB，截断后仍排空输出管道。
- [x] 宿主持有进程树，记录所属根任务、会话、工作区、PID、期限及退出状态；主任务或拥有它的子任务停止时清理进程，宿主退出后重启标记中断并保留日志。
- [x] Windows 使用 portable-pty 的原生 ConPTY，支持交互输入、窗口尺寸调整、Ctrl-C 与按输出标记等待就绪；应答控制台光标位置请求。`readTimedOut` 与 `exited` 分开返回。
- [x] 验收普通管道输入、PTY 输入／中断／调整尺寸、模型继续执行时访问后台 HTTP 服务、日志限额及取消；界面支持查看后台进程日志和取消。

**六、实施顺序与完成条件**

1. 固化已有的两项复现，完成流结束原因与思考块的最小纵向链路。
2. 完成历史回传、检查点恢复、模型能力配置和前端状态展示。
3. 修复 snip/归档边界，将上下文检查接入每次模型请求，再完成语义摘要的原子提交。
4. 接入可配置并发的子任务运行时、队列、结果回收和桌面任务树。
5. 完成长任务对照评估，再实施后台进程与 PTY。

每阶段交付可运行实现、对应回归结果和实际限制。完成后检查旧会话与配置兼容性、停止与恢复、用量统计和 Windows 桌面入口。发布及仓库提交按用户后续指令执行。

**七、本轮交付与验证（2026-11-20）**

- Rust 工作区测试通过；后续块顺序调整后，runtime／shell 针对性测试再次通过，agents 测试通过。Windows shell 与 desktop 构建通过。
- `check-chat-stream.mjs`、`check-ui-controls.mjs`、`check-markdown.mjs` 通过；`check-chat.py`、`check-features.py` 保持通过。
- 新增 `scripts/check-runtime-upgrades.py`：验证思考保存／回传与块顺序、断流部分内容恢复、输出截断不执行工具、工具循环内语义压缩及原文归档、模型窗口诊断、12 个任务按默认 6 并发排队、实际 3／8 并发、父任务取消、单槽嵌套及重启中断标记。
- 本机 Edge 界面验证通过：思考可折叠且作为安全文本显示，后续增量保留既有 DOM 与展开状态，并发设置保存后刷新仍生效，中英文切换、新增 Provider 能力字段保存及子任务菜单正常。
- 用户指定的 `astron-code-latest` 真实接口完成一次有界文件读取任务：工具调用 1 次、正文增量 1 次，返回文件校验标记；首个事件约 4.72 秒，总耗时约 6.07 秒。本次未返回思考字段，思考链路使用本地替身接口验证。凭据仅临时输入与子进程环境传递，未写入项目或报告。
- 后续补齐了后台进程／PTY、请求及压缩指标、跨根恢复、手工子任务控制、显式 fork、推理强度、Provider 共享限额和高并发验收。详细场景与本地调度对照结果见 `dist/validation/runtime-upgrades.json`，其中用量来源是本地替身接口，不代表真实模型性能。
- 用户要求 Windows 客户端 Release，版本为 0.6.0。按最新交付要求改为单文件安装程序 `OpenCowork-0.6.0-Setup-x64.exe`，Release 只保留该安装文件；代码提交并推送至 main，版本标签对齐对应提交。
- [v0.6.0 Windows 安装版](https://github.com/mypengpengli/OpenCowork/releases/tag/v0.6.0) 使用当前用户安装、桌面／开始菜单快捷方式和 Windows 卸载入口；包含经过签名验证的微软 WebView2 引导程序，缺少运行环境时补齐。安装程序通过隔离目录中的安装、覆盖升级、宿主健康检查、保留用户数据卸载及快捷方式／注册项清理验证。
- 149 项 Rust 测试、18 个集成场景、Edge 手工任务创建／补充要求、Windows 原生窗口、完整进程生命周期、日志限额和 30 会话启动检查通过。原生 PTY 就绪检测忽略 ANSI 颜色／光标序列，显式阻塞等待不会触发无进展误判，取消／中断保留请求计时记录。

**八、参考源码与文档**

- [Pi 当前定位及扩展取舍](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/README.md)
- [Pi 模型事件与能力类型](https://github.com/earendil-works/pi/blob/main/packages/ai/src/types.ts)
- [Pi OpenAI 兼容解析与思考回传](https://github.com/earendil-works/pi/blob/main/packages/ai/src/api/openai-completions.ts)
- [Pi AgentSession 的每步压缩接入](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/agent-session.ts)
- [Pi 压缩算法](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/compaction/compaction.ts)
- [Pi 编程代理版本记录](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/CHANGELOG.md)
- [Pi 模型适配版本记录](https://github.com/earendil-works/pi/blob/main/packages/ai/CHANGELOG.md)
- [Pi 子智能体扩展示例](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/examples/extensions/subagent/index.ts)
- [Codex 并发默认值](https://github.com/openai/codex/blob/main/codex-rs/core/src/config/mod.rs)
- [Codex 并发配置计数口径](https://learn.chatgpt.com/docs/config-file/config-reference)
- [Claude Code 子智能体并发与深度](https://code.claude.com/docs/en/sub-agents)
- [DeepSeek Harness 的结构化摘要](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/compaction/compaction-basic/src/summarizer.ts)
- [DeepSeek Harness 的每步压力检查与溢出恢复](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/compaction/compaction-basic/src/index.ts)
- [DeepSeek Harness 的思考事件解析](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/llm/llm-deepseek/src/translate.ts)
- [DeepSeek Harness 的终端生命周期](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/terminal/terminal-bash/src/session.ts)
