# 验证记录与复现

日期：2026-09-16。环境：Windows，单显示器 1920×1080。测试使用临时项目、受控网页和独立配置目录；不修改用户项目。以下本地回归与外部模型实测分别记录。

## 本地回归

- Shell 21 项单元测试通过，包括有后台记忆提取时间戳时的完成事件解析。
- Tools 原有与新增测试通过；其中团队同步 14 项覆盖本地改动保留、双方冲突、过期版本拒绝、412 条件冲突、401/403、4 MiB 响应限制、隐藏记录排除、工作区隔离、切换同步仓库时重置基线与 204 ETag。
- Runtime 会话相关 14 项测试通过。
- UI 控件、流式解析、安全 Markdown 脚本通过；浏览器实测覆盖正文搜索定位、归档深链接、文件第 2 行高亮、惰性冲突审阅与保留本地文件。
- 首次配置与同步设置 API 回归通过：凭据存在但不泄露、非法地址／相对令牌路径拒绝、配置保存、明确配置的凭据缺失报错。
- 工作区、启动、聊天、功能、六方向 17 项本地集成场景通过。功能测试曾暴露时间戳解析错误，已修复并加入回归；另修正了测试提前于后台提取结束进行冲突处理的竞态。
- 原生 Windows 检查通过：Unicode 输入、组合键、Value/Toggle/Expand/Invoke 控件操作、过期状态保护、滚动、窗口身份与模态关闭、窗口被遮挡时的 WGC 截图。

一次启动采样：健康检查约 525 ms，30 个测试会话的 bootstrap 约 26 ms。这不是桌面窗口完整启动耗时或跨产品性能对比。

## 真实模型实测

使用当前配置的 `[编程]gpt-5.4`，Chat Completions 协议。首先执行四次小型能力请求：连接、工具参数、红色测试图识别和流式响应均通过，分别约 3229 / 1895 / 4011 / 1189 ms，提供方报告合计 265 tokens。

固定任务脚本：`scripts/check-real-provider.py`。前台预算为每任务最多 8 次迭代、60000 tokens、240 秒、单次最多 1024 输出 tokens；后台学习与额外提取关闭。表中 token 合计包含提供方报告的缓存输入，不等于计费金额。任务内没有人工纠正；下列“复测”是人工发起的新测试，不能当作首次成功。

| 任务与尝试 | 结果 | 耗时 | 报告 tokens | 证据 |
| --- | --- | --- | --- | --- |
| 读取数字并写入总和，首次 | 成功 | 18.5 s | 6743 | 2 次工具调用，`sum.txt` 实际为 `42` |
| 修改首行并回读，首次 | 失败 | 48.7 s | 4455 | 完成状态失败；首轮报告未保存具体错误，不追溯猜测原因 |
| 修改首行并回读，复测 | 成功 | 42.9 s | 9333 | 3 次工具调用，实际文件内容正确并回读；重复读是任务要求的验证 |
| 浏览器表单，前两次 | 测试配置错误，未完成 | 60.1 / 70.5 s | 11801 / 11841 | fixture 使用 workspace-write，Browser 正确拒绝权限不足；已修正 fixture 权限 |
| 浏览器表单，修正权限后 | 未完成 | 46.3 s | 2198 | 页面打开后 provider_timeout，未自动重放 |
| 浏览器表单，按配置超时复测 | 未完成 | 111.7 s | 6424 | Browser 打开、填写 Ada 均有成功工具结果，随后 provider_connection；未完成点击与结果验证 |

因此不能宣称真实任务全部通过，也不能从本地模型替身推断外部模型任务成功率。真实浏览器端到端案例待提供方连接稳定后复测；混合 DPI 多显示器待具备对应硬件后验证。真实网站、更多模型、更多任务组成的广泛 benchmark 仍未完成。

## 复现命令

```powershell
.\Build-OpenCowork.bat
cargo test --locked --target-dir "$env:LOCALAPPDATA\OpenClaw\target" -p opencowork-shell -p opencowork-tools
cargo test --locked --target-dir "$env:LOCALAPPDATA\OpenClaw\target" -p opencowork-runtime session
node scripts/check-ui-controls.mjs
node scripts/check-chat-stream.mjs
node scripts/check-markdown.mjs
python scripts/check-setup-sync.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe"
python scripts/check-workspaces.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe"
python scripts/check-startup.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe"
python scripts/check-chat.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe"
python scripts/check-features.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe"
python scripts/check-optimizations.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-computer-window.ps1
```

真实模型测试会消耗 API 用量，必须显式加 `--run`，读取当前项目启用的 Provider 配置。令牌只传入子进程环境，不复制到测试文件或报告。输出位置自行选择，不要提交含本地路径的原始报告。可用 `--case browser-form` 只复测浏览器任务。

```powershell
python scripts/check-real-provider.py "$env:LOCALAPPDATA\OpenClaw\target\debug\opencowork-shell.exe" --run --output .tmp/real-provider-report.json
```
