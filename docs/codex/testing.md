# Testing Rules (Codex Quick Reference)

## Purpose

本文件是测试规则摘要，供 Codex 在执行任务时优先读取。
详细说明见 `docs/codex/testing-strategy.md`。

## Required Rules

- 测试改动必须遵循最小改动原则，不引入新的业务语义。
- 测试必须默认离线可运行，不依赖真实 backend/真实网络。
- 每次功能改动必须新增或更新测试（至少覆盖一条成功路径）。
- 每个关键 flow 必须至少有一条失败路径测试，并断言“不变性”。
- 必须更新断言矩阵（涉及 flow 改动时）。
- 仅运行受影响测试命令；非必要不跑全量。

## Required Process

1. 先确认改动边界与受影响 flow。
2. 先补黄金路径，再补失败路径。
3. 用 fixture/helper 复用测试准备逻辑。
4. 记录并汇报执行的测试命令与结果摘要。
5. 按 `docs/codex/checklists/pr-definition-of-done.md` 逐项完成 DoD。

## Command Policy

- 优先运行最小验证集合（受影响 crate/test target）。
- 若需要扩展验证，按风险逐步扩大范围。

## Large-Repo Kickoff (Low Token)

### Pilot Module First

- 大仓库默认先选 1 个高风险模块做样板工程，默认从 `wallet-api` 开始。
- 选择标准：改动频繁 / 线上风险高 / 现有测试薄弱（满足任意两条）。

### Per-Round Limits

- 每轮只处理一个模块、一个 flow。
- 每轮最多 3 个可执行子任务。
- 非目标代码不做“顺便优化”。

## Iteration Order

### Iteration 0 — Quality Gate

- 固定模块级最小命令（smoke + crate check）。
- 固定 PR DoD 勾选流程。
- 清理/忽略真实网络依赖测试（手工运行单独标记）。

### Iteration 1 — Minimal Closure

- 每条 flow 先补：
  - `happy_path_ok`
  - `error_path_no_side_effect`
- 必须包含 DB 状态 + 外部调用记录双断言。

### Iteration 2 — Orchestration Guards

- 补 `orchestration_order_guard` 类型用例。
- 至少新增 2 个“顺序/次数/失败不落库”回归用例。

### Iteration 3 — Reusable Test Infra

- 抽取模块内 `tests/common` fixture/helper。
- 形成可复制模板，用于下一条 flow。

### Iteration 4 — Scale to Next Module

- 同一模板迁移到第二个高风险模块。
- 保持“每轮仅一个模块”策略。

## Round Validation Metrics

每轮只看 4 个指标：

1. 目标 flow 是否有成功+失败覆盖
2. 是否离线稳定跑通（建议连续 3 次）
3. 是否存在不变性断言
4. 是否通过 PR DoD checklist
