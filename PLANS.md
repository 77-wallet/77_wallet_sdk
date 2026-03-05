# PLANS

Current task execution plan.  
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: Large-repo low-token testing kickoff
- Goal: 固定“先稳后扩”的测试覆盖起步流程，默认从 `wallet-api` 试点
- Deliverables:
  - 更新 `docs/codex/testing.md`（加入启动顺序与迭代模型）
  - 更新 `docs/codex/workflows.md`（加入执行硬限制与推荐顺序）

## Scope

### In

- `docs/codex/testing.md`
- `docs/codex/workflows.md`
- `PLANS.md`

### Out

- 业务代码变更
- 测试代码新增/重构

## Constraints

- No new business semantics
- No large refactor
- Offline-test requirement

## Plan

1. Analysis
2. Documentation updates
3. Validation
4. Delivery notes

## Validation Commands

- `rg -n "Large-Repo Kickoff|Iteration 0|Round Validation Metrics" docs/codex/testing.md`
- `rg -n "一次只处理一个模块|Recommended Kickoff Sequence" docs/codex/workflows.md`

## Expected Results

- 文档包含低 token 执行规则与启动顺序
- 执行边界与验证指标可直接复用

## Progress Checklist

- [x] Analysis
- [x] Documentation updates
- [x] Validation
- [x] Delivery notes

## Delivery Notes

- Changed files:
  - `docs/codex/testing.md`
  - `docs/codex/workflows.md`
  - `PLANS.md`
- Key decisions:
  - 默认 `wallet-api` 作为试点模块
  - 每轮只做一个模块/一个 flow/最多 3 子任务
- Risks / follow-ups:
  - 后续落地时需在模块级断言矩阵中持续更新
