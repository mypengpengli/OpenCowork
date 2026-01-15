# Skill 规范与检查清单

本文件描述本系统的 Skill 规范、可选字段与常见错误。需要时再打开阅读。

## 1. 目录结构

```
skill-name/
├── SKILL.md          # 必需
├── scripts/          # 可选（默认 scripts/run.ps1）
├── references/       # 可选（默认 references/REFERENCE.md）
└── assets/           # 可选（默认 assets/template.md）
```

要求：
- 目录名必须与 SKILL.md 的 `name` 完全一致，否则会被系统忽略。
- 目录名仅允许小写字母、数字、连字符。

## 2. SKILL.md 格式（必须）

SKILL.md 必须以 YAML frontmatter 开头，并以独立行的 `---` 结束：

```yaml
---
name: skill-name
description: 描述技能做什么、以及什么时候需要触发（请包含关键词）
---
```

注意：
- `---` 必须独占一行，不能写在 description 末尾同一行。
- 文件需保存为 UTF-8，避免乱码。
- YAML 冒号后必须有空格。

## 3. name 规则（系统校验）

- 1-64 个字符
- 仅允许 `a-z`、`0-9`、`-`
- 不能以 `-` 开头或结尾
- 不能包含 `--` 连续连字符
- 必须与目录名一致

## 4. description 触发原则

description 是技能发现的唯一入口，请写清：
- 这个技能做什么
- 用户可能会怎么说（包含关键词）
- “创建/修改/修复/优化/模板/脚本”等同义词建议覆盖

## 5. 可选 frontmatter 字段（本系统支持）

```yaml
allowed-tools: Read, Write, Edit, Glob, Grep, Bash, run_command, invoke_skill, manage_skill
user-invocable: true
model: gpt-4o-mini
context: 可选上下文提示
metadata:
  author: your-name
  version: "1.0"
```

说明：
- `allowed-tools` 用空格或逗号分隔；省略表示不限制。
- `allowed-tools` 为空表示不允许任何工具。
- `user-invocable` 为 false 时，用户 `/skill` 不可直接调用。

## 6. 资源使用建议

- scripts/：需要确定性执行或批处理时再添加脚本。
- references/：放规范、API、复杂示例，避免把长文塞进 SKILL.md。
- assets/：放模板、素材、示例文件，供复制或引用。

## 7. 常见错误清单

- frontmatter 结束标记不在独立行
- 目录名与 name 不一致
- description 过于模糊导致无法触发
- 只写说明，不提供脚本/模板却声称“可执行”

## 8. 快速检查清单

- [ ] SKILL.md 以 `---` 开头并以独立行 `---` 结束
- [ ] name 合法且与目录名一致
- [ ] description 覆盖触发关键词
- [ ] SKILL.md 正文简洁、流程明确
- [ ] scripts/references/assets 按需创建并与文中引用一致
