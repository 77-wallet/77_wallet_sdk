# Workflows

## Low-Token Task Input Template

每次发给 Codex 的任务输入建议固定 5 段：

1. 目标（1-2句）
2. 改动边界（允许改哪些文件 / 禁止改哪些文件）
3. 必跑命令（精确到 crate/test target）
4. 输出要求（变更清单 + 关键断言 + 命令结果摘要）
5. 文档同步要求（matrix/checklist/plans 是否更新）

建议追加约束语句：

- 只跑受影响测试，不跑全量
- 不要长解释，只给结论和证据
- 新规则优先写到 `docs/codex`，不扩展 root `AGENTS.md`

## Execution Workflow

### Step 1 — Scope

- 确认任务目标与边界
- 更新 `PLANS.md`（非 trivial 任务）

### Step 2 — Implement

- 最小改动实现
- 保持现有接口与行为契约

### Step 3 — Validate

- 仅运行受影响命令
- 记录关键结果摘要与风险点

### Step 4 — Documentation

- 更新断言矩阵（如流程相关）
- 对齐 checklist 与交付说明

## PR Workflow

- PR 描述必须逐条引用 DoD checklist 勾选结果。
- 必须满足：`docs/codex/checklists/pr-definition-of-done.md`
