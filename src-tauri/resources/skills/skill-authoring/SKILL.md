---
name: skill-authoring
description: 编写、修改、审查或修复技能（SKILL.md/技能目录结构）；当用户要求创建/更新/优化技能、补充 scripts/references/assets、修复 frontmatter、校验技能格式或生成技能模板时使用。
---

# 技能编写与维护指南

## 目标
保证技能能被系统发现并正确执行：frontmatter 合规、目录结构完整、描述可触发、资源可用。

## 工作流程
1. 明确需求：技能名称（小写连字符）、用途、触发关键词、输入/输出、是否需要脚本/模板/参考资料。
2. 创建或更新目录：`<skills>/<skill-name>/`，目录名必须与 frontmatter 的 `name` 一致。
3. 编写 frontmatter：仅包含 `name` 和 `description`；起止 `---` 必须独占一行。
4. 编写正文：用命令式语气描述流程与步骤；详细规范移到 references。
5. 处理资源：
   - `scripts/`：需要确定性自动化时，添加脚本并说明如何调用（默认 `scripts/run.ps1`）。
   - `references/`：放规范、API、复杂示例（默认 `references/REFERENCE.md`）。
   - `assets/`：放模板/数据/示例文件（默认 `assets/template.md`）。
6. 自检：按 `references/REFERENCE.md` 的清单检查格式、命名、描述、资源与编码。

## 资源说明
- `references/REFERENCE.md`：完整规范与检查清单。
- `assets/template.md`：SKILL.md 模板，可直接复制并替换占位。
- `scripts/run.ps1`：可选脚本模板；需要自动化时再实现，并用 `run_command` 执行（cwd=技能目录）。

## 最小模板
使用 `assets/template.md` 作为起点，保证描述包含触发关键词。

## 常见失败原因
- frontmatter 结束标记 `---` 不在独立行
- 目录名与 `name` 不一致（会被系统忽略）
- description 过于模糊，导致无法触发
- 只写说明文本，没有实际脚本/资源
