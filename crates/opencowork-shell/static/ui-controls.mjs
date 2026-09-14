// UI-only helpers: preserve drafts and bind labels without rebuilding forms.
export function appendDraft(current, instruction, prepend = false) {
  if (!current.trim()) return instruction
  if (current.includes(instruction.trim())) return current
  return prepend ? `${instruction.trim()}\n\n${current}` : `${current}\n\n${instruction}`
}

export const executionRanges = {
  maxIterations: [1, 1000], maxTokens: [100, 10000000], maxSeconds: [1, 86400],
  repeatedResults: [2, 20], maxOutputTokens: [128, 131072],
}

const english = {
  '目标、补充要求与消息队列': 'Goal, steering and message queue',
  '把本次消息设为目标': 'Use this message as a goal',
  '把本次消息设为目标并自动继续': 'Use this message as a goal and continue automatically',
  '发送目标后，在预算内持续执行并检查完成证据。': 'Work within the budget and verify completion after sending a goal.',
  '运行中补充要求，或排入下一轮': 'Steer the active task or queue the next message',
  '暂停目标': 'Pause goal', '恢复目标': 'Resume goal', '补充当前任务': 'Steer current task', '排入下一轮': 'Queue next message',
  '移除': 'Remove', '尚未设置目标': 'No goal set', '请先发送一条消息创建会话': 'Send a message to create a session first',
  '接管电脑并停止任务': 'Take over and stop tasks', '允许继续电脑操作': 'Resume computer control',
  '已暂停电脑操作，可以手动接管。': 'Computer control paused. You can take over.',
  '已恢复电脑操作权限；从新观察继续。': 'Computer control resumed; observe again before acting.',
  '自动执行与模型能力': 'Execution and model capabilities', '高级设置：预算与后台模型': 'Advanced: budgets and background model',
  '每轮最多迭代': 'Iterations per turn', '每轮 token 上限': 'Tokens per turn', '每轮秒数上限': 'Seconds per turn',
  '重复结果停止阈值': 'Repeated result limit', '单次最大输出 token': 'Output tokens per request',
  '后台模型名称（留空跟随当前模型）': 'Background model (blank uses current model)', '后台模型': 'Background model',
  '推理强度': 'Reasoning effort', '推理强度：提供方默认': 'Reasoning: provider default', '无': 'None', '最少': 'Minimal', '低': 'Low', '中': 'Medium', '高': 'High',
  '浏览器工具': 'Browser tool', '生成技能候选': 'Generate skill candidates', '自动采用技能': 'Automatically adopt skills',
  '启用独立浏览器工具（需安装 Chrome 或 Edge）': 'Enable browser tool (Chrome or Edge required)',
  '成功流程生成技能候选（额外后台请求）': 'Generate skill candidates from successful workflows (background requests)',
  '自动采用候选技能（关闭时先审阅）': 'Automatically adopt candidates (otherwise review first)',
  '保存执行设置': 'Save execution settings', '已保存，新回合生效。': 'Saved. Applies to new turns.',
  '检测模型连接、工具、图像和流式能力': 'Check connection, tools, vision and streaming',
  '审阅更改与恢复': 'Review changes and restore', '选择或输入工作区文件路径': 'Choose or enter a workspace file path',
  '暂存整个文件': 'Stage file', '取消整个文件暂存': 'Unstage file', '撤销工作区文本更改': 'Revert working text changes',
  '未暂存': 'Unstaged', '已暂存': 'Staged', '取消暂存此块': 'Unstage hunk', '暂存此块': 'Stage hunk', '撤销此块': 'Revert hunk',
  '恢复此版本': 'Restore version', '读取差异': 'Load diff', '查看恢复点': 'View restore points',
  '历史全文搜索': 'Full-text history search', '搜索正文、工具结果或归档': 'Search messages, tool results or archives',
  '所有项目': 'All projects', '包含其他项目及旧会话': 'Include other projects and old sessions', '搜索': 'Search', '没有匹配结果': 'No matches',
  '查看匹配消息原文': 'View matching message', '历史已变化，请重新搜索': 'History changed. Search again.',
  '记忆来源与冲突': 'Memory sources and conflicts', '查看新信息来源': 'View new source', '保留原记忆': 'Keep existing memory',
  '采用新事实': 'Use new fact', '暂无自动记忆或待处理冲突': 'No automatic memories or pending conflicts', '查看来源与冲突': 'View sources and conflicts',
  '可复用技能候选': 'Reusable skill candidates', '当前版本': 'Current version', '候选版本': 'Candidate version', '采用': 'Adopt', '拒绝': 'Reject', '停用': 'Disable', '刷新候选': 'Refresh candidates',
  '定时任务与结果收件箱': 'Scheduled tasks and inbox', '到时执行的任务': 'Task to run', '间隔分钟；填 0 使用每日时间': 'Interval in minutes; 0 for daily time',
  '每日执行时间': 'Daily time', 'IANA 时区': 'IANA timezone', '错过后补一次': 'Catch up once', '暂停': 'Pause', '取消当前执行': 'Cancel running task', '恢复': 'Resume',
  '本地收件箱': 'Local inbox', '错过执行后补一次': 'Catch up once after a missed run', '创建定时任务': 'Create scheduled task', '刷新任务与收件箱': 'Refresh tasks and inbox',
  '程序运行时执行。默认跳过错过的任务；异常中断后暂停，先检查结果再恢复。': 'Runs while the app is open. Missed runs are skipped by default; interrupted tasks pause for review.',
  '隔离工作区': 'Isolated workspace', '任务名称（英文、数字、短横线）': 'Task name (letters, digits, hyphens)', '创建独立工作区': 'Create worktree', '刷新列表': 'Refresh list',
  '从已提交版本创建独立 Git worktree；当前未提交更改保留在原工作区。新路径可作为独立任务的工作目录。': 'Create a worktree from the committed version. Uncommitted changes stay in the original workspace. Use the new path for a separate task.',
}

export function createUi(state, status) {
  const pairs = new Map(Object.entries(english).map(([zh, en]) => [zh, [zh, en]]))
  for (const pair of [...pairs.values()]) pairs.set(pair[1], pair)
  const tr = (zh, en = english[zh] || zh) => {
    pairs.set(zh, [zh, en]); pairs.set(en, [zh, en])
    return state.locale === 'en' ? en : zh
  }
  function bind(node, value, attr) {
    const pair = pairs.get(value)
    if (attr) node.setAttribute(attr, pair ? tr(...pair) : value)
    else node.textContent = pair ? tr(...pair) : value
    if (pair) node.dataset[`ui${attr === 'placeholder' ? 'Placeholder' : attr === 'aria-label' ? 'Aria' : 'Text'}`] = JSON.stringify(pair)
    return node
  }
  function localize(root) {
    for (const n of root.querySelectorAll('[data-ui-text], [data-ui-placeholder], [data-ui-aria]')) {
      for (const [key, attr] of [['uiText', null], ['uiPlaceholder', 'placeholder'], ['uiAria', 'aria-label']]) {
        if (!n.dataset[key]) continue
        const pair = JSON.parse(n.dataset[key]), current = attr ? n.getAttribute(attr) : n.textContent
        // A dynamic result may have replaced the original label; leave it intact.
        if (pair.includes(current)) { if (attr) n.setAttribute(attr, tr(...pair)); else n.textContent = tr(...pair) }
      }
    }
  }
  const el = (tag, text = '', cls = '') => { const n = document.createElement(tag); n.className = cls; return bind(n, text) }
  function button(text, action) {
    const n = el('button', text, 'ghost-button'); n.type = 'button'
    n.onclick = async () => {
      if (n.disabled) return
      n.disabled = true; n.dataset.busy = 'true'; n.setAttribute('aria-busy', 'true')
      const card = n.closest('.settings-card')
      let feedback = card?.querySelector('[data-action-feedback]')
      if (card && !feedback) { feedback = el('p', '', 'settings-card-copy'); feedback.dataset.actionFeedback = ''; feedback.setAttribute('role', 'status'); card.append(feedback) }
      if (feedback) { feedback.textContent = tr('正在处理…', 'Working…'); feedback.dataset.tone = 'default' }
      try {
        await action()
        if (feedback) feedback.textContent = tr('操作完成', 'Done')
      } catch (e) {
        if (feedback) { feedback.textContent = tr(e.message); feedback.dataset.tone = 'error' }
        else status(tr(e.message), true)
      } finally { n.disabled = false; delete n.dataset.busy; n.removeAttribute('aria-busy'); n.dispatchEvent(new CustomEvent('ui:action-done', { bubbles: true })) }
    }
    return n
  }
  return { tr, el, bind, localize, button }
}
