# Testing Rules (Codex Quick Reference)

## Purpose

本文件是测试规则摘要，供 Codex 在执行任务时优先读取。
详细说明见 `docs/codex/testing-strategy.md`。
可复制测试模板见 `docs/codex/testing-templates.md`。

## Required Rules

- 测试改动必须遵循最小改动原则，不引入新的业务语义。
- 测试必须默认离线可运行，不依赖真实 backend/真实网络。
- 测试必须按 unit / component / integration / smoke-live 分层，不能把真实环境测试混入默认测试。
- 每次功能改动必须新增或更新测试（至少覆盖一条成功路径）。
- 每个关键 flow 必须至少有一条失败路径测试，并断言“不变性”。
- 必须更新断言矩阵（涉及 flow 改动时）。
- 仅运行受影响测试命令；非必要不跑全量。

## Layer Rules

- Unit：只测一个函数、规则或步骤；禁止真实网络、固定 `test_data`、`WalletManager::new()`、全局 `CONTEXT`。
- Component：允许临时 SQLite + migration + 真实 repo/dao；禁止真实 backend/链 RPC；必须断言真实落库结果。
- Integration：使用 fake/mock backend、fake/mock chain、本地临时数据和统一 fixture；必须断言返回值、DB、外部调用和副作用。
- Smoke/Live：允许真实 backend/真实链路，但必须显式标记并手动运行，默认测试和 CI 主链不运行。

## Directory Rules

- 路径必须表达测试层级、业务模块、具体 flow；不要把多类测试混在一个大 `mod.rs`。
- Unit / component 默认贴近被测代码；当一个模块超过 3 个测试或出现多个 flow 时，拆到同名目录下的 `*_tests.rs`。
- Integration / smoke 统一放在 crate 的 `tests/` 下，
  按 `integration/<module>/`、`smoke/<module>/` 拆分。
- `tests/harness/` 只放真正跨模块复用的 test harness、fake、fixture、assertion；
  禁止 `withdraw` 依赖 `collect` 私有 helper。
- 新增测试优先用清晰文件名表达意图，例如 `confirm_tests.rs`、`resource_gate.rs`、`live_backend.rs`。

目录契约：

```text
<crate>/
  src/...                         # unit / component tests live near code
  tests/
    harness/                      # cross-module test environment and fakes
    integration/<business_module>/<flow>.rs
    smoke/<business_module>/<flow>.rs
```

命名契约：

- `<business_module>` 使用业务归属，例如 `api_wallet`、`transaction`、`stake`。
- `<flow>.rs` 使用具体流程或风险点，例如 `withdraw_notification.rs`、`collect_resource_gate.rs`。
- 一个 integration 文件只覆盖同一类业务流程；如果同时出现 fee、notification、receipt，应拆成多个文件。
- `harness` 是专业测试术语，表示测试夹具/执行环境总成；它只能承载通用环境、fake、fixture、assertion，不能变成新的业务模块。

## Required Process

1. 先确认改动边界与受影响 flow。
2. 先补黄金路径，再补失败路径。
3. 用 fixture/helper 复用测试准备逻辑。
4. 记录并汇报执行的测试命令与结果摘要。
5. 按 `docs/codex/checklists/pr-definition-of-done.md` 逐项完成 DoD。

## Command Policy

- 优先运行最小验证集合（受影响 crate/test target）。
- 若需要扩展验证，按风险逐步扩大范围。
- 默认稳定测试优先使用 `cargo test --workspace --no-default-features`。
- 标准集成测试显式使用 `--features integration-tests`。
- 真实环境 smoke/live 测试必须使用独立 feature 或 `#[ignore]`。

过渡期注意：当前 `wallet-api` default feature 仍包含 `integration-tests`。
在拆分完成前，不要把普通 `cargo test -p wallet-api` 视为纯单元测试。

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

- 抽取模块内 `tests/harness` fixture/helper。
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
