# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database test-first convergence (batch B3: api_wallet fee)
- Goal:
  - 在不改生产逻辑前提下，为 `api_wallet/fee` 补齐成功、边界、回滚三类仓库测试
  - 继续以确定性、离线可运行为准

## Scope

### In

- `wallet-database/src/repositories/api_wallet/fee.rs`
- `PLANS.md`

### Out

- 仓储 API/事务抽象改造
- DAO SQL 语义调整
- 其它 `api_wallet/*` 仓库测试扩展

## Constraints

- Test-only changes; no production behavior changes
- Offline validation only
- Deterministic tests only (no flaky stress patterns)

## Plan

1. 为 `api_wallet/fee` 增加成功、边界、回滚三类测试
2. 事务回滚场景使用 `ApiFeeDao` + 显式 rollback 断言不落库
3. 执行最小验证命令并记录结果

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database fee_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] Add fee success/edge/rollback tests
- [x] Run focused offline validation
