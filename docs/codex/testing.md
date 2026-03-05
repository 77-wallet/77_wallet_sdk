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
