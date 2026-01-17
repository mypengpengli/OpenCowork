import { computed } from 'vue'
import { useLocaleStore, type Locale } from './stores/locale'

type Messages = Record<string, string>

const messages: Record<Locale, Messages> = {
  zh: {
    'app.name': 'OpenCowork',
    'app.shortName': 'OC',
    'menu.chat': '对话',
    'menu.history': '历史',
    'menu.settings': '设置',
    'sidebar.newChat': '新对话',
    'sidebar.conversations': '对话记录',
    'sidebar.empty': '暂无对话',
    'sidebar.deleteConfirm': '确定删除该对话吗？此操作不可恢复。',
    'language.english': 'English',
    'language.chinese': '中文',
    'common.unknown': '未知',
    'common.new': '新建',
    'common.save': '保存',
    'common.delete': '删除',
    'common.copy': '复制',
    'common.edit': '编辑',
    'common.enable': '启用',
    'common.cancel': '取消',
    'common.create': '创建',
    'common.refresh': '刷新',
    'common.clear': '清空',
    'common.start': '开始',
    'common.stop': '停止',
    'main.status.capturing': '监控中',
    'main.status.paused': '已暂停',
    'main.status.records': '记录',
    'main.buttons.history': '历史对话',
    'main.buttons.loadAlerts': '加载今天提醒',
    'main.empty.title': 'OpenCowork',
    'main.empty.desc': '???????????"??"???????????????????',
    'main.empty.item1': '????????',
    'main.empty.item2': '???????? 10 ?????',
    'main.empty.item3': '???????????????',
    'main.empty.tip': '????????????? Skill ??????????????????????? Skill ??????',
    'main.loading': '思考中...',
    'main.progress.title': '后台过程',
    'main.progress.running': '处理中',
    'main.progress.done': '已完成',
    'main.progress.error': '出错',
    'main.progress.empty': '暂无步骤',
    'main.progress.expand': '展开',
    'main.progress.collapse': '收起',
    'main.skill.empty': '没有匹配的技能',
    'main.input.placeholder': '输入你的问题... (输入 / 查看可用技能)',
    'main.attachments.add': '添加附件',
    'main.attachments.remove': '移除附件',
    'main.attachments.limit': '附件数量已达上限',
    'main.attachmentOnly': '已添加附件',
    'main.tools.mode.title': '工具执行权限',
    'main.tools.mode.desc': '首次使用工具需要选择执行模式。你可以在设置中随时修改。',
    'main.tools.mode.whitelist': '白名单模式（推荐）',
    'main.tools.mode.allowAll': '全允许模式（风险较高）',
    'main.tools.mode.hint': '白名单模式下只允许设置中的命令和目录执行。',
    'main.alert.noneToday': '今天没有历史提醒',
    'main.alert.loaded': '已加载今天 {{count}} 条提醒',
    'main.alert.loadFailed': '加载今天提醒失败: {{error}}',
    'main.chat.newConfirm': '确定新建对话吗？当前对话将被清空。',
    'main.chat.newSuccess': '已新建对话',
    'main.chat.saved': '对话已保存: {{title}}',
    'main.chat.saveEmpty': '没有可保存的对话内容',
    'main.chat.loaded': '对话已加载',
    'main.chat.clearConfirm': '确定清空当前对话吗？',
    'main.chat.error': '错误: {{error}}',
    'main.chat.cancelled': '已停止当前请求',
    'main.chat.invokingSkill': '🔧 正在调用技能 `/{skill}`...',
    'alert.detectedTitle': '⚠️ **检测到问题**',
    'alert.typeLine': '**类型**: {{type}}',
    'alert.messageLine': '**信息**: {{message}}',
    'alert.suggestionLine': '**建议**: {{suggestion}}',
    'settings.tabs.profiles': '配置方案',
    'settings.tabs.skills': '技能管理',
    'settings.header.profiles': '配置方案',
    'settings.locale.systemValue': '系统语言(get_system_locale): {{value}}',
    'settings.locale.systemError': '系统语言读取失败: {{error}}',
    'settings.header.skills': '技能管理',
    'settings.buttons.newProfile': '新建方案',
    'settings.buttons.openSkillsFolder': '打开技能文件夹',
    'settings.buttons.newSkill': '新建技能',
    'settings.buttons.checkUpdate': '检查更新',
    'settings.update.openFailed': '打开更新页面失败: {{error}}',
    'settings.loading.profiles': '正在加载方案...',
    'settings.loading.skills': '正在加载技能...',
    'settings.empty.profiles': '暂无配置方案',
    'settings.empty.profilesHint': '点击"新建方案"创建一个',
    'settings.empty.skills': '暂无可用技能',
    'settings.empty.skillsHint': '点击"新建技能"创建一个，或在技能文件夹中添加 SKILL.md 文件',
    'settings.empty.skillsDir': '技能文件夹: {{dir}}',
    'settings.profile.active': '当前使用',
    'settings.profile.readFailed': '读取失败',
    'settings.profile.loadFailed': '读取方案失败: {{error}}',
    'settings.profile.loadConfigFailed': '加载当前配置失败: {{error}}',
    'settings.profile.saveSuccess': '方案已保存',
    'settings.profile.saveFailed': '保存方案失败: {{error}}',
    'settings.profile.enableSuccess': '方案已启用',
    'settings.profile.enableFailed': '启用方案失败: {{error}}',
    'settings.profile.deleteConfirm': '确定删除方案 "{{name}}" 吗？',
    'settings.profile.deleteSuccess': '方案已删除',
    'settings.profile.deleteFailed': '删除方案失败: {{error}}',
    'settings.profile.nameRequired': '请输入方案名称',
    'settings.profile.drawer.new': '新建方案',
    'settings.profile.drawer.edit': '编辑方案',
    'settings.profile.drawer.copy': '复制方案',
    'settings.connection.success': '连接成功',
    'settings.connection.failed': '连接失败: {{error}}',
    'settings.skills.createSuccess': '技能创建成功',
    'settings.skills.createFailed': '技能创建失败',
    'settings.skills.deleteConfirm': '确定删除技能 "{{name}}" 吗？',
    'settings.skills.deleteSuccess': '技能已删除',
    'settings.skills.deleteFailed': '删除技能失败',
    'settings.skills.nameRequired': '请输入技能名称',
    'settings.skills.descRequired': '请输入技能描述',
    'settings.skills.openDirCopied': '技能文件夹路径已复制到剪贴板: {{dir}}',
    'settings.skills.openDirInfo': '技能文件夹: {{dir}}',
    'settings.skills.help.title': '使用说明',
    'settings.skills.help.item1': '在聊天框中输入 <code>/技能名</code> 即可调用技能',
    'settings.skills.help.item2': '例如：<code>/export 今天</code> 导出今天的屏幕活动记录',
    'settings.skills.help.item3': '技能会自动出现在 AI 的提示中，AI 会在合适的时候建议使用',
    'settings.form.profileInfo': '方案信息',
    'settings.form.profileName': '方案名称',
    'settings.form.profileNamePlaceholder': '例如：工作/本地模型/写代码',
    'settings.form.modelConfig': '模型配置',
    'settings.form.modelProvider': '模型来源',
    'settings.form.apiType': 'API 类型',
    'settings.form.apiEndpoint': 'API 地址',
    'settings.form.apiKey': 'API Key',
    'settings.form.modelName': '模型名称',
    'settings.form.ollamaEndpoint': 'Ollama 地址',
    'settings.form.captureConfig': '截屏配置',
    'settings.form.captureEnable': '启用监控',
    'settings.form.captureInterval': '截屏间隔',
    'settings.form.captureIntervalUnit': '毫秒',
    'settings.form.compressQuality': '压缩质量',
    'settings.form.skipUnchanged': '跳过无变化',
    'settings.form.skipUnchangedTip': '启用后，当画面无明显变化时跳过识别，节省Token消耗',
    'settings.form.changeThreshold': '变化敏感度',
    'settings.form.changeThresholdUnit': '相似度',
    'settings.form.changeThresholdTip': '相似度阈值，越高越容易跳过（0.95表示95%相似就跳过）',
    'settings.form.recentSummaryLimit': '近期摘要条数',
    'settings.form.countUnit': '条',
    'settings.form.recentSummaryTip': '截图分析时带入最近的摘要条数（1-100）',
    'settings.form.recentDetailLimit': '近期 detail 条数',
    'settings.form.recentDetailTip': '截图分析时带入最近的 detail 条数（0 表示不带）',
    'settings.form.alertConfidence': '提醒置信度阈值',
    'settings.form.confidenceUnit': '置信度',
    'settings.form.alertConfidenceTip': '有问题且置信度高于阈值时，自动在对话框提示建议',
    'settings.form.alertCooldown': '提醒冷却时间',
    'settings.form.secondsUnit': '秒',
    'settings.form.alertCooldownTip': '相同问题在冷却时间内不重复提示，避免刷屏',
    'settings.form.storageConfig': '存储配置',
    'settings.form.uiConfig': '界面配置',
    'settings.form.showProcess': '显示后台过程',
    'settings.form.showProcessTip': '显示模型/工具在后台执行的步骤，完成后自动折叠',
    'settings.form.toolsConfig': '工具权限',
    'settings.form.toolsMode': '执行模式',
    'settings.tools.mode.unset': '首次询问',
    'settings.tools.mode.whitelist': '白名单',
    'settings.tools.mode.allowAll': '全允许',
    'settings.form.toolsAllowedCommands': '允许命令',
    'settings.form.toolsAllowedCommandsPlaceholder': '每行一个命令，例如: python, ffmpeg, agent-browser',
    'settings.form.toolsAllowedDirs': '允许目录',
    'settings.form.toolsAllowedDirsPlaceholder': '每行一个目录，例如: C:\\work\\files',
    'settings.form.retentionDays': '保留天数',
    'settings.form.daysUnit': '天',
    'settings.form.contextSize': '上下文大小',
    'settings.form.charsUnit': '字符',
    'settings.form.contextSizeTip': '对话时加载的历史记录最大字符数，越大越详细但消耗更多Token',

    'settings.form.contextMode': '对话上下文模式',
    'settings.form.contextModeTip': '自动：仅在提到屏幕/历史/截图等时加载；总是：每次都加载；关闭：不加载',
    'settings.form.contextMode.auto': '自动',
    'settings.form.contextMode.always': '总是',
    'settings.form.contextMode.off': '关闭',
    'settings.form.contextDetailHours': 'detail 时间窗',
    'settings.form.contextDetailHoursTip': '仅带入最近 N 小时的 detail，0 表示不带 detail',
    'settings.form.hoursUnit': '小时',
    'settings.form.autoClear': '启动时清空历史',
    'settings.form.autoClearTip': '开启后每次启动自动清空历史记录',
    'settings.form.testConnection': '测试连接',
    'settings.form.saveProfile': '保存方案',
    'settings.form.provider.api': 'API (云端)',
    'settings.form.provider.ollama': 'Ollama (本地)',
    'settings.form.api.custom': '自定义',
    'settings.skills.modal.title': '新建技能',
    'settings.skills.modal.name': '技能名称',
    'settings.skills.modal.namePlaceholder': '小写字母、数字和连字符，如 my-skill',
    'settings.skills.modal.description': '技能描述',
    'settings.skills.modal.descriptionPlaceholder': '描述技能的功能和使用场景',
    'settings.skills.modal.instructions': '技能指令',
    'settings.skills.modal.instructionsPlaceholder': 'Markdown 格式的技能指令',
    'settings.skills.modal.cancel': '取消',
    'settings.skills.modal.create': '创建',
    'settings.skills.templateLabel': '技能模板',
    'settings.skills.template.basic': '通用模板',
    'settings.skills.template.file': '文件自动化',
    'settings.skills.template.web': '网页自动化',
    'settings.skills.template.doc': '文档导出',
    'settings.skills.templateChangeConfirm': '切换模板会覆盖当前内容，是否继续？',
    'settings.skills.templateContent.basic': `# 技能名称

## 使用场景
描述何时使用此技能。

## 输入与输出
- 输入: ...
- 输出: ...

## 执行步骤
1. 第一步
2. 第二步
3. 第三步

## 资源目录
- scripts/: 可执行脚本（默认: scripts/run.ps1）
- references/: 参考资料（默认: references/REFERENCE.md）
- assets/: 模板或数据（默认: assets/template.md）

## 自动化说明
需要自动化时，用 Bash 或 run_command 运行 scripts/run.ps1，并把 cwd 设置为技能目录。`,
    'settings.skills.templateContent.file': `# 文件处理技能

## 使用场景
批量读取、筛选、修改或汇总文件内容。

## 输入与输出
- 输入: 文件/目录路径与筛选条件
- 输出: 处理后的文件或汇总报告

## 执行步骤
1. 用 Glob/Grep 找到目标文件或内容。
2. 用 Read/Write/Edit 处理内容。
3. 如需批量处理，在 scripts/run.ps1 中实现逻辑并通过 Bash/run_command 执行。

## 资源目录
- scripts/: 可执行脚本（默认: scripts/run.ps1）
- references/: 参考资料（默认: references/REFERENCE.md）
- assets/: 模板或数据（默认: assets/template.md）`,
    'settings.skills.templateContent.web': `# 网页自动化技能

## 使用场景
自动填写表单、抓取网页信息、截图或验证页面状态。

## 输入与输出
- 输入: 目标网址与操作步骤
- 输出: 抓取结果或截图

## 执行步骤
1. 通过脚本或浏览器工具完成页面操作。
2. 将关键步骤或元素说明写入 references/REFERENCE.md。
3. 需要模板或截图时放入 assets/。

## 资源目录
- scripts/: 可执行脚本（默认: scripts/run.ps1）
- references/: 参考资料（默认: references/REFERENCE.md）
- assets/: 模板或数据（默认: assets/template.md）`,
    'settings.skills.templateContent.doc': `# 文档导出技能

## 使用场景
将对话或数据导出为 Word/PDF/Markdown 等文档。

## 输入与输出
- 输入: 原始文本或数据
- 输出: 目标文档文件

## 执行步骤
1. 使用 assets/template.md 作为输出模板或结构。
2. 在 scripts/run.ps1 中实现生成逻辑。
3. 输出文档保存到指定目录。

## 资源目录
- scripts/: 可执行脚本（默认: scripts/run.ps1）
- references/: 参考资料（默认: references/REFERENCE.md）
- assets/: 模板或数据（默认: assets/template.md）`,
    'settings.skills.defaultInstructions': `# 技能名称

## 使用场景
描述何时使用此技能。

## 输入与输出
- 输入: ...
- 输出: ...

## 执行步骤
1. 第一步
2. 第二步
3. 第三步

## 资源目录
- scripts/: 可执行脚本（默认: scripts/run.ps1）
- references/: 参考资料（默认: references/REFERENCE.md）
- assets/: 模板或数据（默认: assets/template.md）

## 自动化说明
需要自动化时，用 Bash 或 run_command 运行 scripts/run.ps1，并把 cwd 设置为技能目录。`,
    // 全局提示词
    'settings.tabs.prompts': '全局提示词',
    'settings.header.prompts': '全局提示词',
    'settings.buttons.newPrompt': '新建提示词',
    'settings.empty.prompts': '暂无全局提示词',
    'settings.empty.promptsHint': '添加常用信息（如公司、部门、领导姓名），AI 对话时会自动使用',
    'settings.prompt.saveSuccess': '提示词已保存',
    'settings.prompt.saveFailed': '保存提示词失败: {{error}}',
    'settings.prompt.deleteConfirm': '确定删除提示词 "{{name}}" 吗？',
    'settings.prompt.deleteSuccess': '提示词已删除',
    'settings.prompt.modal.titleNew': '新建提示词',
    'settings.prompt.modal.titleEdit': '编辑提示词',
    'settings.prompt.modal.name': '名称',
    'settings.prompt.modal.namePlaceholder': '如：个人信息、公司信息',
    'settings.prompt.modal.content': '内容',
    'settings.prompt.modal.contentPlaceholder': '如：我是XX公司的员工，部门经理是张三，分管领导是李四',
    'settings.prompt.help.title': '使用说明',
    'settings.prompt.help.item1': '全局提示词会在每次 AI 对话时自动注入',
    'settings.prompt.help.item2': '适合保存个人信息、公司信息、常用格式等',
    'settings.prompt.help.item3': '可以创建多条提示词，通过开关控制是否启用',
    'history.title': '历史记录',
    'history.refresh': '刷新',
    'history.openScreenshots': '打开截图文件夹',
    'history.clearDay': '清空当天',
    'history.clearAll': '清空全部',
    'history.empty': '暂无记录',
    'history.status.issue': '有问题',
    'history.status.ok': '正常',
    'history.confidence': '置信度 {{value}}',
    'history.detail': '详情',
    'history.issueSummary': '问题摘要',
    'history.suggestion': '建议',
    'history.drawer.title': '详情',
    'history.drawer.time': '时间',
    'history.drawer.app': '应用',
    'history.drawer.status': '状态',
    'history.drawer.issueType': '问题类型',
    'history.drawer.confidence': '置信度',
    'history.drawer.issueSummary': '问题摘要',
    'history.drawer.suggestion': '建议',
    'history.drawer.screenshot': '截图',
    'history.detailLabel': '详情',
    'history.detailEmpty': '无详情',
    'history.clearConfirm': '确定清空当前日期的历史记录吗？',
    'history.clearAllConfirm': '确定清空所有历史记录吗？此操作不可恢复。',
    'history.clearSuccess': '已清空 {{count}} 条记录',
    'history.clearFailed': '清空失败: {{error}}',
    'history.openScreenshotsFailed': '打开截图文件夹失败: {{error}}',
    'capture.autoRestarting': '监控意外暂停，正在尝试自动恢复...',
    'capture.autoRestored': '监控已自动恢复',
    'capture.autoRestoreFailed': '自动恢复失败: {{error}}',
    'chat.defaultTitle': '新对话',
    'message.role.alert': '警告',
    'message.role.user': '你',
    'message.role.assistant': '助手',
  },
  en: {
    'app.name': 'OpenCowork',
    'app.shortName': 'OC',
    'menu.chat': 'Chat',
    'menu.history': 'History',
    'menu.settings': 'Settings',
    'sidebar.newChat': 'New chat',
    'sidebar.conversations': 'Chats',
    'sidebar.empty': 'No conversations',
    'sidebar.deleteConfirm': 'Delete this conversation? This cannot be undone.',
    'language.english': 'English',
    'language.chinese': '中文',
    'common.unknown': 'Unknown',
    'common.new': 'New',
    'common.save': 'Save',
    'common.delete': 'Delete',
    'common.copy': 'Duplicate',
    'common.edit': 'Edit',
    'common.enable': 'Enable',
    'common.cancel': 'Cancel',
    'common.create': 'Create',
    'common.refresh': 'Refresh',
    'common.clear': 'Clear',
    'common.start': 'Start',
    'common.stop': 'Stop',
    'main.status.capturing': 'Monitoring',
    'main.status.paused': 'Paused',
    'main.status.records': 'Records',
    'main.buttons.history': 'Saved chats',
    'main.buttons.loadAlerts': "Load today's alerts",
    'main.empty.title': 'OpenCowork',
    'main.empty.desc': "I'm your work assistant. Click \"Start\" to record your screen activity. You can ask me anytime:",
    'main.empty.item1': 'What did I just do?',
    'main.empty.item2': 'Review what I did in the last 10 minutes',
    'main.empty.item3': 'Which file did I just edit?',
    'main.empty.tip': 'I can also help organize files, invoke Skills to handle file operations or auto-fill web forms, and generate the Skills you need to get tasks done.',
    'main.loading': 'Thinking...',
    'main.progress.title': 'Background progress',
    'main.progress.running': 'Running',
    'main.progress.done': 'Done',
    'main.progress.error': 'Error',
    'main.progress.empty': 'No steps yet',
    'main.progress.expand': 'Expand',
    'main.progress.collapse': 'Collapse',
    'main.skill.empty': 'No matching skills',
    'main.input.placeholder': 'Ask a question... (type / to view skills)',
    'main.attachments.add': 'Attach',
    'main.attachments.remove': 'Remove attachment',
    'main.attachments.limit': 'Attachment limit reached',
    'main.attachmentOnly': 'Attachment(s) added',
    'main.tools.mode.title': 'Tool Execution Permissions',
    'main.tools.mode.desc': 'Choose a tool execution mode the first time tools are used. You can change this in settings later.',
    'main.tools.mode.whitelist': 'Whitelist mode (recommended)',
    'main.tools.mode.allowAll': 'Allow all (higher risk)',
    'main.tools.mode.hint': 'Whitelist mode only allows commands and directories configured in settings.',
    'main.alert.noneToday': 'No alerts today',
    'main.alert.loaded': 'Loaded {{count}} alerts today',
    'main.alert.loadFailed': "Failed to load today's alerts: {{error}}",
    'main.chat.newConfirm': 'Start a new conversation? Current chat will be cleared.',
    'main.chat.newSuccess': 'New conversation started',
    'main.chat.saved': 'Conversation saved: {{title}}',
    'main.chat.saveEmpty': 'No conversation to save',
    'main.chat.loaded': 'Conversation loaded',
    'main.chat.clearConfirm': 'Clear the current conversation?',
    'main.chat.error': 'Error: {{error}}',
    'main.chat.cancelled': 'Request cancelled',
    'main.chat.invokingSkill': '🔧 Calling skill `/{skill}`...',
    'alert.detectedTitle': '⚠️ **Issue detected**',
    'alert.typeLine': '**Type**: {{type}}',
    'alert.messageLine': '**Message**: {{message}}',
    'alert.suggestionLine': '**Suggestion**: {{suggestion}}',
    'settings.tabs.profiles': 'Profiles',
    'settings.tabs.skills': 'Skills',
    'settings.header.profiles': 'Profiles',
    'settings.locale.systemValue': 'System locale (get_system_locale): {{value}}',
    'settings.locale.systemError': 'Failed to read system locale: {{error}}',
    'settings.header.skills': 'Skills',
    'settings.buttons.newProfile': 'New Profile',
    'settings.buttons.openSkillsFolder': 'Open Skills Folder',
    'settings.buttons.newSkill': 'New Skill',
    'settings.buttons.checkUpdate': 'Check Updates',
    'settings.update.openFailed': 'Failed to open updates page: {{error}}',
    'settings.loading.profiles': 'Loading profiles...',
    'settings.loading.skills': 'Loading skills...',
    'settings.empty.profiles': 'No profiles yet',
    'settings.empty.profilesHint': 'Click "New Profile" to create one',
    'settings.empty.skills': 'No skills available',
    'settings.empty.skillsHint': 'Click "New Skill" to create one, or add a SKILL.md file in the skills folder',
    'settings.empty.skillsDir': 'Skills folder: {{dir}}',
    'settings.profile.active': 'Active',
    'settings.profile.readFailed': 'Load failed',
    'settings.profile.loadFailed': 'Failed to load profile: {{error}}',
    'settings.profile.loadConfigFailed': 'Failed to load current config: {{error}}',
    'settings.profile.saveSuccess': 'Profile saved',
    'settings.profile.saveFailed': 'Failed to save profile: {{error}}',
    'settings.profile.enableSuccess': 'Profile enabled',
    'settings.profile.enableFailed': 'Failed to enable profile: {{error}}',
    'settings.profile.deleteConfirm': 'Delete profile "{{name}}"?',
    'settings.profile.deleteSuccess': 'Profile deleted',
    'settings.profile.deleteFailed': 'Failed to delete profile: {{error}}',
    'settings.profile.nameRequired': 'Please enter a profile name',
    'settings.profile.drawer.new': 'New Profile',
    'settings.profile.drawer.edit': 'Edit Profile',
    'settings.profile.drawer.copy': 'Duplicate Profile',
    'settings.connection.success': 'Connection successful',
    'settings.connection.failed': 'Connection failed: {{error}}',
    'settings.skills.createSuccess': 'Skill created',
    'settings.skills.createFailed': 'Failed to create skill',
    'settings.skills.deleteConfirm': 'Delete skill "{{name}}"?',
    'settings.skills.deleteSuccess': 'Skill deleted',
    'settings.skills.deleteFailed': 'Failed to delete skill',
    'settings.skills.nameRequired': 'Please enter a skill name',
    'settings.skills.descRequired': 'Please enter a skill description',
    'settings.skills.openDirCopied': 'Skills folder path copied to clipboard: {{dir}}',
    'settings.skills.openDirInfo': 'Skills folder: {{dir}}',
    'settings.skills.help.title': 'How to use',
    'settings.skills.help.item1': 'Type <code>/skill-name</code> in the chat to invoke a skill',
    'settings.skills.help.item2': 'Example: <code>/export today</code> exports today\'s screen activity',
    'settings.skills.help.item3': 'Skills appear in AI prompts, and the assistant may suggest them when appropriate',
    'settings.form.profileInfo': 'Profile Info',
    'settings.form.profileName': 'Profile Name',
    'settings.form.profileNamePlaceholder': 'e.g. Work / Local Model / Coding',
    'settings.form.modelConfig': 'Model',
    'settings.form.modelProvider': 'Model Source',
    'settings.form.apiType': 'API Type',
    'settings.form.apiEndpoint': 'API Endpoint',
    'settings.form.apiKey': 'API Key',
    'settings.form.modelName': 'Model Name',
    'settings.form.ollamaEndpoint': 'Ollama Endpoint',
    'settings.form.captureConfig': 'Capture',
    'settings.form.captureEnable': 'Enable Monitoring',
    'settings.form.captureInterval': 'Capture Interval',
    'settings.form.captureIntervalUnit': 'ms',
    'settings.form.compressQuality': 'Compression Quality',
    'settings.form.skipUnchanged': 'Skip Unchanged',
    'settings.form.skipUnchangedTip': 'When enabled, skips analysis if the screen has not changed to save tokens',
    'settings.form.changeThreshold': 'Change Sensitivity',
    'settings.form.changeThresholdUnit': 'Similarity',
    'settings.form.changeThresholdTip': 'Similarity threshold; higher values skip more (0.95 means 95% similarity)',
    'settings.form.recentSummaryLimit': 'Recent Summary Count',
    'settings.form.countUnit': 'items',
    'settings.form.recentSummaryTip': 'Number of recent summaries included during analysis (1-100)',
    'settings.form.recentDetailLimit': 'Recent Detail Count',
    'settings.form.recentDetailTip': 'Number of recent details included during analysis (0 means none)',
    'settings.form.alertConfidence': 'Alert Confidence Threshold',
    'settings.form.confidenceUnit': 'Confidence',
    'settings.form.alertConfidenceTip': 'Show alerts when issues are detected with confidence above this threshold',
    'settings.form.alertCooldown': 'Alert Cooldown',
    'settings.form.secondsUnit': 's',
    'settings.form.alertCooldownTip': 'Avoid repeated alerts for the same issue during the cooldown period',
    'settings.form.storageConfig': 'Storage',
    'settings.form.uiConfig': 'UI',
    'settings.form.showProcess': 'Show background progress',
    'settings.form.showProcessTip': 'Show backend steps for model/tools and auto-collapse when done',
    'settings.form.toolsConfig': 'Tool Permissions',
    'settings.form.toolsMode': 'Execution Mode',
    'settings.tools.mode.unset': 'Ask on first use',
    'settings.tools.mode.whitelist': 'Whitelist',
    'settings.tools.mode.allowAll': 'Allow all',
    'settings.form.toolsAllowedCommands': 'Allowed commands',
    'settings.form.toolsAllowedCommandsPlaceholder': 'One per line, e.g. python, ffmpeg, agent-browser',
    'settings.form.toolsAllowedDirs': 'Allowed directories',
    'settings.form.toolsAllowedDirsPlaceholder': 'One per line, e.g. C:\\work\\files',
    'settings.form.retentionDays': 'Retention Days',
    'settings.form.daysUnit': 'days',
    'settings.form.contextSize': 'Context Size',
    'settings.form.charsUnit': 'chars',
    'settings.form.contextSizeTip': 'Max characters loaded into context; larger means more detail but more tokens',
    'settings.form.contextMode': 'Context Mode',
    'settings.form.contextModeTip': 'Auto loads only when screen/history terms appear; Always loads every time; Off disables screen context',
    'settings.form.contextMode.auto': 'Auto',
    'settings.form.contextMode.always': 'Always',
    'settings.form.contextMode.off': 'Off',
    'settings.form.contextDetailHours': 'Detail Window',
    'settings.form.contextDetailHoursTip': 'Include detail only from the last N hours (0 means none)',
    'settings.form.hoursUnit': 'hours',

    'settings.form.autoClear': 'Clear History on Start',
    'settings.form.autoClearTip': 'Automatically clear history on startup',
    'settings.form.testConnection': 'Test Connection',
    'settings.form.saveProfile': 'Save Profile',
    'settings.form.provider.api': 'API (Cloud)',
    'settings.form.provider.ollama': 'Ollama (Local)',
    'settings.form.api.custom': 'Custom',
    'settings.skills.modal.title': 'New Skill',
    'settings.skills.modal.name': 'Skill Name',
    'settings.skills.modal.namePlaceholder': 'Lowercase letters, numbers, and dashes, e.g. my-skill',
    'settings.skills.modal.description': 'Skill Description',
    'settings.skills.modal.descriptionPlaceholder': 'Describe the skill and its use cases',
    'settings.skills.modal.instructions': 'Skill Instructions',
    'settings.skills.modal.instructionsPlaceholder': 'Markdown formatted instructions',
    'settings.skills.modal.cancel': 'Cancel',
    'settings.skills.modal.create': 'Create',
    'settings.skills.templateLabel': 'Skill template',
    'settings.skills.template.basic': 'General',
    'settings.skills.template.file': 'File automation',
    'settings.skills.template.web': 'Web automation',
    'settings.skills.template.doc': 'Document export',
    'settings.skills.templateChangeConfirm': 'Switching templates will replace the current content. Continue?',
    'settings.skills.templateContent.basic': `# Skill Name

## When to use
Describe when this skill should be used.

## Inputs and outputs
- Input: ...
- Output: ...

## Steps
1. Step one
2. Step two
3. Step three

## Resources
- scripts/: executable scripts (default: scripts/run.ps1)
- references/: reference docs (default: references/REFERENCE.md)
- assets/: templates or data (default: assets/template.md)

## Automation
Run scripts/run.ps1 via Bash or run_command and set cwd to the skill directory when automation is needed.`,
    'settings.skills.templateContent.file': `# File Automation Skill

## When to use
Batch read, filter, edit, or summarize files.

## Inputs and outputs
- Input: file/directory paths and filters
- Output: processed files or summary reports

## Steps
1. Use Glob/Grep to locate target files or content.
2. Use Read/Write/Edit to process content.
3. For batch work, implement logic in scripts/run.ps1 and run it with Bash/run_command.

## Resources
- scripts/: executable scripts (default: scripts/run.ps1)
- references/: reference docs (default: references/REFERENCE.md)
- assets/: templates or data (default: assets/template.md)`,
    'settings.skills.templateContent.web': `# Web Automation Skill

## When to use
Auto-fill forms, extract web data, take screenshots, or verify page states.

## Inputs and outputs
- Input: target URLs and steps
- Output: extracted data or screenshots

## Steps
1. Use a script or browser tools to drive the page.
2. Record key steps and element notes in references/REFERENCE.md.
3. Store templates or snapshots in assets/.

## Resources
- scripts/: executable scripts (default: scripts/run.ps1)
- references/: reference docs (default: references/REFERENCE.md)
- assets/: templates or data (default: assets/template.md)`,
    'settings.skills.templateContent.doc': `# Document Export Skill

## When to use
Export conversations or data to Word/PDF/Markdown files.

## Inputs and outputs
- Input: source text or data
- Output: target document files

## Steps
1. Use assets/template.md as the output template or structure.
2. Implement generation logic in scripts/run.ps1.
3. Save outputs to the target directory.

## Resources
- scripts/: executable scripts (default: scripts/run.ps1)
- references/: reference docs (default: references/REFERENCE.md)
- assets/: templates or data (default: assets/template.md)`,
    'settings.skills.defaultInstructions': `# Skill Name

## When to use
Describe when this skill should be used.

## Inputs and outputs
- Input: ...
- Output: ...

## Steps
1. Step one
2. Step two
3. Step three

## Resources
- scripts/: executable scripts (default: scripts/run.ps1)
- references/: reference docs (default: references/REFERENCE.md)
- assets/: templates or data (default: assets/template.md)

## Automation
Run scripts/run.ps1 via Bash or run_command and set cwd to the skill directory when automation is needed.`,
    // Global Prompts
    'settings.tabs.prompts': 'Global Prompts',
    'settings.header.prompts': 'Global Prompts',
    'settings.buttons.newPrompt': 'New Prompt',
    'settings.empty.prompts': 'No global prompts',
    'settings.empty.promptsHint': 'Add frequently used info (company, department, manager names) to auto-include in AI conversations',
    'settings.prompt.saveSuccess': 'Prompt saved',
    'settings.prompt.saveFailed': 'Failed to save prompt: {{error}}',
    'settings.prompt.deleteConfirm': 'Delete prompt "{{name}}"?',
    'settings.prompt.deleteSuccess': 'Prompt deleted',
    'settings.prompt.modal.titleNew': 'New Prompt',
    'settings.prompt.modal.titleEdit': 'Edit Prompt',
    'settings.prompt.modal.name': 'Name',
    'settings.prompt.modal.namePlaceholder': 'e.g. Personal Info, Company Info',
    'settings.prompt.modal.content': 'Content',
    'settings.prompt.modal.contentPlaceholder': 'e.g. I work at XX Company, my manager is John, my director is Jane',
    'settings.prompt.help.title': 'How to use',
    'settings.prompt.help.item1': 'Global prompts are automatically injected into every AI conversation',
    'settings.prompt.help.item2': 'Great for storing personal info, company details, common formats, etc.',
    'settings.prompt.help.item3': 'Create multiple prompts and toggle them on/off as needed',
    'history.title': 'History',
    'history.refresh': 'Refresh',
    'history.openScreenshots': 'Open Screenshots Folder',
    'history.clearDay': 'Clear Day',
    'history.clearAll': 'Clear All',
    'history.empty': 'No records',
    'history.status.issue': 'Issue',
    'history.status.ok': 'OK',
    'history.confidence': 'Confidence {{value}}',
    'history.detail': 'Details',
    'history.issueSummary': 'Issue Summary',
    'history.suggestion': 'Suggestion',
    'history.drawer.title': 'Details',
    'history.drawer.time': 'Time',
    'history.drawer.app': 'App',
    'history.drawer.status': 'Status',
    'history.drawer.issueType': 'Issue Type',
    'history.drawer.confidence': 'Confidence',
    'history.drawer.issueSummary': 'Issue Summary',
    'history.drawer.suggestion': 'Suggestion',
    'history.drawer.screenshot': 'Screenshot',
    'history.detailLabel': 'Detail',
    'history.detailEmpty': 'No detail',
    'history.clearConfirm': 'Clear history for this date?',
    'history.clearAllConfirm': 'Clear all history? This cannot be undone.',
    'history.clearSuccess': 'Cleared {{count}} records',
    'history.clearFailed': 'Clear failed: {{error}}',
    'history.openScreenshotsFailed': 'Failed to open screenshots folder: {{error}}',
    'capture.autoRestarting': 'Monitoring stopped unexpectedly. Attempting auto-restart...',
    'capture.autoRestored': 'Monitoring has been restored',
    'capture.autoRestoreFailed': 'Auto-restart failed: {{error}}',
    'chat.defaultTitle': 'New Conversation',
    'message.role.alert': 'Alert',
    'message.role.user': 'You',
    'message.role.assistant': 'Assistant',
  },
}

function formatMessage(template: string, params?: Record<string, string | number>) {
  if (!params) {
    return template
  }
  return template.replace(/\{\{(\w+)\}\}/g, (_, key) => String(params[key] ?? ''))
}

export function translate(locale: Locale, key: string, params?: Record<string, string | number>) {
  const table = messages[locale] || messages.en
  const template = table[key] || messages.en[key] || key
  return formatMessage(template, params)
}

export function useI18n() {
  const localeStore = useLocaleStore()
  const locale = computed(() => localeStore.locale)
  const t = (key: string, params?: Record<string, string | number>) =>
    translate(localeStore.locale, key, params)

  return {
    locale,
    t,
    setLocale: localeStore.setLocale,
    toggleLocale: localeStore.toggleLocale,
  }
}

export function localeToDateLocale(locale: Locale) {
  return locale === 'zh' ? 'zh-CN' : 'en-US'
}
